import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/endpoints.dart';
import '../../core/config/lightwalletd_presets.dart';
import '../../core/config/network_config.dart';
import '../../core/errors/sync_failure.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/server_failover_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/endpoints.dart' as rust;

/// The host shown for an endpoint URL.
String endpointHost(String url) => Uri.tryParse(url)?.authority ?? url;

/// Tries `url` for `kind`: a relay `GET /health`, or lightwalletd's info (network and
/// tip). Returns null when it answers, else what went wrong (user copy).
Future<String?> tryEndpoint(EndpointKind kind, String url) async {
  try {
    switch (kind) {
      case EndpointKind.relay:
        await rust.checkRelay(relayUrl: url);
      case EndpointKind.lightwalletd:
        await rust.checkLightwalletd(
          lightwalletdUrl: url,
          networkName: kZafeNetwork,
        );
    }
    return null;
  } catch (e) {
    final failure = classifySyncFailure(
      e,
      fallback: kind == EndpointKind.relay
          ? SyncEndpoint.relay
          : SyncEndpoint.lightwalletd,
    );
    return switch (failure.kind) {
      SyncFailureKind.relayUnreachable ||
      SyncFailureKind.lightwalletdUnreachable ||
      SyncFailureKind.offline => 'Couldn\'t connect to this server.',
      SyncFailureKind.tls =>
        'The secure connection failed: the certificate isn\'t trusted, has '
            'expired or names another server.',
      SyncFailureKind.timeout => 'The server didn\'t answer in time.',
      SyncFailureKind.wrongNetwork =>
        'This server is on another network than this app ($kZafeNetwork).',
      SyncFailureKind.updateRequired ||
      SyncFailureKind.relayOutdated ||
      // "Use Tor" is on: the test goes through Tor too, never directly.
      SyncFailureKind.torConnecting ||
      SyncFailureKind.torFailed => zafeErrorMessage(e),
      _ =>
        kind == EndpointKind.relay
            ? 'This doesn\'t look like a Zafe server.'
            : 'This doesn\'t look like a Zcash server.',
    };
  }
}

/// The public servers and the custom one, with what each answered, in one card. Plain
/// data, so it can be rendered without Rust (`tool/screens/servers_render_test.dart`).
class LightwalletdServerList extends StatelessWidget {
  const LightwalletdServerList({
    super.key,
    required this.presets,
    required this.selectedUrl,
    required this.probes,
    required this.onSelect,
    required this.onCustom,
    this.customOpen = false,
    this.busyUrl,
  });

  final List<LightwalletdPreset> presets;
  final String selectedUrl;
  final Map<String, ServerProbe> probes;
  final ValueChanged<LightwalletdPreset>? onSelect;
  final VoidCallback? onCustom;

  /// The custom address field is open under the list.
  final bool customOpen;

  /// The server being tried after a tap.
  final String? busyUrl;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final custom = lightwalletdPresetFor(selectedUrl, presets) == null;
    Widget selectedMark(bool selected) => SizedBox(
      width: 20,
      child: selected
          ? AppIcon(AppIcons.check, size: 20, color: colors.icon.accent)
          : null,
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final preset in presets)
            MobileListRow(
              leading: selectedMark(preset.url == selectedUrl),
              label: preset.label,
              value: busyUrl == preset.url
                  ? 'Connecting...'
                  : (probes[preset.url] ?? ServerProbe.checking).label,
              valueColor:
                  switch ((probes[preset.url] ?? ServerProbe.checking).check) {
                    ServerCheck.unavailable ||
                    ServerCheck.wrongNetwork => colors.text.destructive,
                    _ => colors.text.secondary,
                  },
              minRowHeight: 48,
              onTap: onSelect == null ? null : () => onSelect!(preset),
            ),
          MobileListRow(
            leading: selectedMark(custom),
            label: 'Custom server',
            value: custom && !customOpen ? endpointHost(selectedUrl) : null,
            valueColor: colors.text.secondary,
            minRowHeight: 48,
            onTap: onCustom,
          ),
        ],
      ),
    );
  }
}

/// Settings > Zcash server (this phone's, for every vault): the public servers, each
/// checked when the screen opens, picked with a tap; "Custom server" opens an address
/// field right here, tested before it is saved.
class ZcashServerScreen extends ConsumerStatefulWidget {
  const ZcashServerScreen({super.key});

  @override
  ConsumerState<ZcashServerScreen> createState() => _ZcashServerScreenState();
}

class _ZcashServerScreenState extends ConsumerState<ZcashServerScreen> {
  final _probes = <String, ServerProbe>{};
  String? _busyUrl;
  String? _error;
  late bool _customOpen =
      lightwalletdPresetFor(ref.read(endpointsProvider).lightwalletdUrl) ==
      null;
  late final _url = TextEditingController(
    text: _customOpen ? ref.read(endpointsProvider).lightwalletdUrl : '',
  );
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    for (final preset in kLightwalletdPresets) {
      unawaited(
        probeLightwalletd(preset.url).then((probe) {
          if (mounted) setState(() => _probes[preset.url] = probe);
        }),
      );
    }
  }

  @override
  void dispose() {
    _url.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Saves `url` once it answered, then syncs with it.
  Future<bool> _use(String url, {String? label}) async {
    setState(() {
      _busyUrl = url;
      _error = null;
    });
    final problem = await tryEndpoint(EndpointKind.lightwalletd, url);
    if (!mounted) return false;
    if (problem != null) {
      setState(() {
        _busyUrl = null;
        _error = label == null ? problem : '$label: $problem';
        if (label != null) {
          _probes[url] = const ServerProbe(ServerCheck.unavailable);
        }
      });
      return false;
    }
    await ref
        .read(endpointsProvider.notifier)
        .set(EndpointKind.lightwalletd, url);
    if (!mounted) return true;
    setState(() => _busyUrl = null);
    unawaited(_resync());
    return true;
  }

  Future<void> _resync() async {
    if (!ref.read(vaultProvider).hasVault) return;
    await ref.read(proposalsProvider.notifier).refreshFresh();
    await ref.read(vaultProvider.notifier).sync();
  }

  Future<void> _select(LightwalletdPreset preset) async {
    if (preset.url == ref.read(endpointsProvider).lightwalletdUrl) return;
    setState(() => _customOpen = false);
    await _use(preset.url, label: preset.label);
  }

  Future<void> _saveCustom() async {
    final check = checkEndpointUrl(_url.text);
    if (check.url == null) {
      setState(() => _error = check.error);
      throw const ActionCancelled();
    }
    // A server of someone else's choosing sees this phone's activity: with a vault on
    // the phone, ask the owner.
    if (ref.read(vaultProvider).vaults.isNotEmpty &&
        !await confirmUnlock(
          context,
          ref,
          reason: 'Confirm it\'s you to change the server',
        )) {
      throw const ActionCancelled();
    }
    if (!await _use(check.url!)) throw const ActionCancelled();
  }

  Future<void> _reset() async {
    await ref.read(endpointsProvider.notifier).reset(EndpointKind.lightwalletd);
    if (!mounted) return;
    setState(() {
      _customOpen = false;
      _error = null;
    });
    unawaited(_resync());
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final endpoints = ref.watch(endpointsProvider);
    final busy = _busyUrl != null;
    final isDefault = endpoints.isDefault(EndpointKind.lightwalletd);
    return ZafeScreen(
      title: 'Zcash server',
      bottom: _customOpen || !isDefault
          ? Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_customOpen)
                  ActionButton(
                    label: 'Test and save',
                    busyLabel: 'Connecting...',
                    doneLabel: 'Saved',
                    resetAfterDone: true,
                    errorFallback: 'Couldn\'t save the server.',
                    onPressed: _saveCustom,
                  ),
                if (!isDefault) ...[
                  if (_customOpen) const SizedBox(height: AppSpacing.xs),
                  AppButton(
                    expand: true,
                    variant: AppButtonVariant.ghost,
                    onPressed: busy ? null : _reset,
                    child: const Text('Reset to default'),
                  ),
                ],
              ],
            )
          : null,
      children: [
        LightwalletdServerList(
          presets: kLightwalletdPresets,
          selectedUrl: endpoints.lightwalletdUrl,
          probes: _probes,
          busyUrl: _busyUrl,
          customOpen: _customOpen,
          onSelect: busy ? null : _select,
          onCustom: busy
              ? null
              : () => setState(() {
                  _customOpen = true;
                  _focus.requestFocus();
                }),
        ),
        if (_customOpen) ...[
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            controller: _url,
            focusNode: _focus,
            enabled: !busy,
            hintText: 'https://',
            keyboardType: TextInputType.url,
            textInputAction: TextInputAction.done,
            inputFormatters: [LengthLimitingTextInputFormatter(200)],
          ),
        ],
        if (_error != null) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            _error!,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.destructive,
            ),
          ),
        ],
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Zafe reads the Zcash blockchain from this server. Every vault on this '
          'phone uses it.',
          style: AppTypography.bodySmall.copyWith(color: colors.text.secondary),
        ),
      ],
    );
  }
}

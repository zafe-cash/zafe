import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/endpoints.dart';
import '../../core/config/lightwalletd_presets.dart';
import '../../core/config/network_config.dart';
import '../../core/errors/sync_failure.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../notifications/vault_watch.dart' show reregisterPush;
import '../../providers/endpoints_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/server_failover_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/endpoints.dart' as rust;

/// Edits the relay or lightwalletd URL: checks the format, tries a connection, then saves
/// (for every vault on this phone). "Reset to default" goes back to the build's URL.
/// The Zcash server opens on the list of public servers first ([kLightwalletdPresets]),
/// with a custom URL one tap away.
Future<void> showEndpointSheet(
  BuildContext context,
  WidgetRef ref,
  EndpointKind kind,
) async {
  final changed = await showAppMobileSheet<bool>(
    context: context,
    builder: (_) => kind == EndpointKind.lightwalletd
        ? const _LightwalletdSheet()
        : const _EndpointSheet(kind: EndpointKind.relay),
  );
  if (changed == true) {
    if (kind == EndpointKind.relay) unawaited(reregisterPush());
    // Pick up the new server right away.
    await ref.read(proposalsProvider.notifier).refresh();
    await ref.read(vaultProvider.notifier).sync();
  }
}

/// The relay and Zcash server, before any vault exists (Settings is only reachable from
/// Home). Each row opens the editor above.
Future<void> showServerSettingsSheet(
  BuildContext context,
  WidgetRef ref,
) async {
  final kind = await showAppMobileSheet<EndpointKind>(
    context: context,
    builder: (_) => const _ServerSettingsSheet(),
  );
  if (kind != null && context.mounted) {
    await showEndpointSheet(context, ref, kind);
  }
}

class _ServerSettingsSheet extends ConsumerWidget {
  const _ServerSettingsSheet();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final endpoints = ref.watch(endpointsProvider);
    final style = AppTypography.labelLarge.copyWith(
      fontWeight: FontWeight.w400,
      color: colors.text.accent,
    );
    Widget row(EndpointKind kind, String label, String value) => MobileListRow(
      leading: AppIcon(AppIcons.endpoint, size: 20, color: colors.icon.muted),
      label: label,
      value: value,
      minRowHeight: 44,
      textStyle: style,
      valueTextStyle: style,
      valueColor: colors.text.accent,
      chevronColor: colors.icon.accent,
      showChevron: true,
      onTap: () => Navigator.of(context).pop(kind),
    );
    return MobileModalScaffold(
      title: 'Server settings',
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Every vault on this phone uses these.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          row(
            EndpointKind.relay,
            'Relay',
            endpoints.relayIsPlaceholder
                ? 'Not configured'
                : endpointHost(endpoints.relayUrl),
          ),
          row(
            EndpointKind.lightwalletd,
            'Zcash server',
            endpointHost(endpoints.lightwalletdUrl),
          ),
        ],
      ),
    );
  }
}

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
            ? 'This doesn\'t look like a Zafe relay.'
            : 'This doesn\'t look like a lightwalletd server.',
    };
  }
}

/// The public servers and the custom one, with what each answered. Plain data, so it can
/// be rendered without Rust (`tool/screens/servers_render_test.dart`).
class LightwalletdServerList extends StatelessWidget {
  const LightwalletdServerList({
    super.key,
    required this.presets,
    required this.selectedUrl,
    required this.probes,
    required this.onSelect,
    required this.onCustom,
    this.busyUrl,
  });

  final List<LightwalletdPreset> presets;
  final String selectedUrl;
  final Map<String, ServerProbe> probes;
  final ValueChanged<LightwalletdPreset>? onSelect;
  final VoidCallback? onCustom;

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
    return Column(
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
            valueColor: switch ((probes[preset.url] ?? ServerProbe.checking)
                .check) {
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
          value: custom ? endpointHost(selectedUrl) : null,
          valueColor: colors.text.secondary,
          minRowHeight: 48,
          showChevron: true,
          onTap: onCustom,
        ),
      ],
    );
  }
}

/// Picks the Zcash server from the public list (each one checked when the sheet opens) or
/// opens the custom URL editor.
class _LightwalletdSheet extends ConsumerStatefulWidget {
  const _LightwalletdSheet();

  @override
  ConsumerState<_LightwalletdSheet> createState() => _LightwalletdSheetState();
}

class _LightwalletdSheetState extends ConsumerState<_LightwalletdSheet> {
  final _probes = <String, ServerProbe>{};
  String? _busyUrl;
  String? _error;

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

  Future<void> _select(LightwalletdPreset preset) async {
    if (preset.url == ref.read(endpointsProvider).lightwalletdUrl) {
      Navigator.of(context).pop(false);
      return;
    }
    setState(() {
      _busyUrl = preset.url;
      _error = null;
    });
    final problem = await tryEndpoint(EndpointKind.lightwalletd, preset.url);
    if (!mounted) return;
    if (problem != null) {
      setState(() {
        _busyUrl = null;
        _error = '${preset.label}: $problem';
        _probes[preset.url] = const ServerProbe(ServerCheck.unavailable);
      });
      return;
    }
    await ref
        .read(endpointsProvider.notifier)
        .set(EndpointKind.lightwalletd, preset.url);
    if (mounted) Navigator.of(context).pop(true);
  }

  Future<void> _custom() async {
    final changed = await showAppMobileSheet<bool>(
      context: context,
      builder: (_) => const _EndpointSheet(kind: EndpointKind.lightwalletd),
    );
    if (changed == true && mounted) Navigator.of(context).pop(true);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final busy = _busyUrl != null;
    return MobileModalScaffold(
      title: 'Zcash server',
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'The lightwalletd server this phone reads the $kZafeNetwork chain '
            'from. It sees which blocks this phone downloads, not the vault\'s '
            'keys.${kLightwalletdPresets.length > 1 ? ' If a listed server stops answering, Zafe moves to the next one.' : ''}',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          LightwalletdServerList(
            presets: kLightwalletdPresets,
            selectedUrl: ref.watch(endpointsProvider).lightwalletdUrl,
            probes: _probes,
            busyUrl: _busyUrl,
            onSelect: busy ? null : _select,
            onCustom: busy ? null : _custom,
          ),
          if (_error != null) ...[
            const SizedBox(height: AppSpacing.xs),
            Text(
              _error!,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.destructive,
              ),
            ),
          ],
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Checking the list contacts each server once (through Tor when '
            'it\'s on).',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ),
    );
  }
}

class _EndpointSheet extends ConsumerStatefulWidget {
  const _EndpointSheet({required this.kind});
  final EndpointKind kind;

  @override
  ConsumerState<_EndpointSheet> createState() => _EndpointSheetState();
}

class _EndpointSheetState extends ConsumerState<_EndpointSheet> {
  late final _url = TextEditingController(
    text: ref.read(endpointsProvider).url(widget.kind),
  );
  final _focus = FocusNode();
  bool _busy = false;
  String? _error;

  bool get _relay => widget.kind == EndpointKind.relay;

  @override
  void dispose() {
    _url.dispose();
    _focus.dispose();
    super.dispose();
  }

  Future<void> _save(String input) async {
    final check = checkEndpointUrl(input);
    if (check.url == null) {
      setState(() => _error = check.error);
      return;
    }
    final url = check.url!;
    setState(() {
      _busy = true;
      _error = null;
    });
    final problem = await tryEndpoint(widget.kind, url);
    if (!mounted) return;
    if (problem != null) {
      setState(() {
        _busy = false;
        _error = problem;
      });
      return;
    }
    await ref.read(endpointsProvider.notifier).set(widget.kind, url);
    if (mounted) Navigator.of(context).pop(true);
  }

  Future<void> _reset() async {
    await ref.read(endpointsProvider.notifier).reset(widget.kind);
    if (mounted) Navigator.of(context).pop(true);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final isDefault = ref.watch(endpointsProvider).isDefault(widget.kind);
    final defaultUrl = ZafeEndpoints.defaults.url(widget.kind);
    return MobileModalScaffold(
      title: _relay ? 'Relay' : 'Zcash server',
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            _relay
                ? 'Carries proposals, votes and signatures between members, '
                      'encrypted. Every member of a vault must use the same relay.'
                : 'The lightwalletd server this phone reads the $kZafeNetwork '
                      'chain from. It sees which blocks this phone downloads, not '
                      'the vault\'s keys.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          MobileTextField(
            controller: _url,
            focusNode: _focus,
            enabled: !_busy,
            hintText: 'https://',
            keyboardType: TextInputType.url,
            textInputAction: TextInputAction.done,
            inputFormatters: [LengthLimitingTextInputFormatter(200)],
            onSubmitted: _busy ? null : _save,
          ),
          if (_error != null) ...[
            const SizedBox(height: AppSpacing.xs),
            Text(
              _error!,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.destructive,
              ),
            ),
          ],
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Default: $defaultUrl',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            onPressed: _busy ? null : () => _save(_url.text),
            child: Text(_busy ? 'Connecting...' : 'Test and save'),
          ),
          if (!isDefault) ...[
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _busy ? null : _reset,
              child: const Text('Reset to default'),
            ),
          ],
        ],
      ),
    );
  }
}

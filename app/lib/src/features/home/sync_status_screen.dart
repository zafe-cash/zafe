import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/diagnostics/crash_log.dart';
import '../../core/errors/sync_failure.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/config/relay_presets.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_confirm.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_relay.dart';
import '../../rust/api/vault.dart' as rust_vault;
import '../settings/relay_picker.dart';

SyncFailure? currentSyncFailure(WidgetRef ref) {
  final syncError = ref.watch(vaultProvider.select((v) => v.syncError));
  final relayError = ref.watch(proposalsProvider.select((p) => p.error));
  return homeSyncFailure(syncError: syncError, relayError: relayError);
}

DateTime? relayUnreachableSince(String vaultId) {
  try {
    return fromUnixSeconds(
      rust_vault.vaultRelayStatus(vaultId: vaultId).unreachableSince,
    );
  } catch (_) {
    return null;
  }
}

const kSuggestMoveAfter = Duration(hours: 6);

Future<String?> suggestedServer(
  String current, {
  List<RelayPreset>? presets,
  Future<RelayCheckResult> Function(String url) check = checkRelayAt,
}) async {
  for (final preset in presets ?? kRelayPresets) {
    if (sameRelayUrl(preset.url, current)) continue;
    if ((await check(preset.url)).usable) return preset.url;
  }
  return null;
}

class SyncStatusScreen extends ConsumerStatefulWidget {
  const SyncStatusScreen({super.key, this.retry});

  final Future<void> Function()? retry;

  @override
  ConsumerState<SyncStatusScreen> createState() => _SyncStatusScreenState();
}

class _SyncStatusScreenState extends ConsumerState<SyncStatusScreen> {
  bool _retrying = false;

  DateTime? _savedSyncedAt;

  String? _suggested;
  bool _asked = false;

  @override
  void initState() {
    super.initState();
    final id = ref.read(vaultProvider).activeId;
    if (id != null) {
      VaultSummaries.read(id).then((s) {
        if (mounted) setState(() => _savedSyncedAt = s.syncedAt);
      });
    }
  }

  String? _repaired;

  Future<void> _restore() async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Restore the vault on the server',
    )) {
      throw const ActionCancelled();
    }
    final entries = await ref.read(proposalsProvider.notifier).restoreRelay();
    if (!mounted) return;
    setState(
      () => _repaired = entries == 0
          ? 'The server already has everything this phone has.'
          : 'Restored $entries entries on the server.',
    );
  }

  Future<void> _follow() async {
    if (!await showAppConfirm(
      context,
      title: followRelayTitle,
      body: followRelayWarning,
      confirmLabel: 'Follow the server',
    )) {
      throw const ActionCancelled();
    }
    if (!mounted) throw const ActionCancelled();
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Follow the server\'s history',
    )) {
      throw const ActionCancelled();
    }
    final dropped = await ref.read(proposalsProvider.notifier).followRelay();
    if (mounted) setState(() => _repaired = followRelayDone(dropped));
  }

  Future<void> _retry() async {
    setState(() => _retrying = true);
    try {
      final tor = ref.read(torProvider);
      if (tor.failed) await ref.read(torProvider.notifier).retry();
      final retry = widget.retry;
      if (retry != null) {
        await retry();
      } else {
        await ref.read(proposalsProvider.notifier).refreshFresh();
        await ref.read(vaultProvider.notifier).sync(force: true);
      }
    } finally {
      if (mounted) setState(() => _retrying = false);
    }
  }

  Future<void> _moveTo(String url) async {
    final name =
        ref.read(activeVaultNameProvider) ??
        ref.read(vaultProvider).summary?.name ??
        'the vault';
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Move $name to ${relayHost(url)}',
    )) {
      throw const ActionCancelled();
    }
    await ref.read(relayMoveProvider.notifier).start(url);
    if (mounted) unawaited(context.push('/vault-server'));
  }

  void _suggest(String current) {
    if (_asked) return;
    _asked = true;
    suggestedServer(current).then((url) {
      if (mounted) setState(() => _suggested = url);
    });
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final failure = currentSyncFailure(ref);
    final vault = ref.watch(vaultProvider);
    final endpoints = ref.watch(endpointsProvider);
    final vaultRelay = ref.watch(activeVaultRelayProvider)?.url;
    final moving = ref.watch(relayMoveProvider.select((m) => m.moving));
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    final refreshedAt = ref.watch(
      proposalsProvider.select((p) => p.refreshedAt),
    );
    final syncedAt =
        (vault.activeId == null ? null : vault.syncedAt[vault.activeId]) ??
        _savedSyncedAt;

    final syncing = failure == null && (vault.syncing || _retrying);
    final firstSync = syncing && vault.firstSync;
    final downSince =
        failure?.kind == SyncFailureKind.relayUnreachable &&
            vault.activeId != null
        ? relayUnreachableSince(vault.activeId!)
        : null;
    final longOutage =
        downSince != null &&
        DateTime.now().difference(downSince) >= kSuggestMoveAfter &&
        !moving;
    if (longOutage && vaultRelay != null) _suggest(vaultRelay);
    final suggested = longOutage ? _suggested : null;

    final body = <Widget>[];
    body.add(
      Text(
        failure?.title ??
            (firstSync
                ? 'First sync'
                : syncing
                ? 'Syncing'
                : 'Vault in sync'),
        style: AppTypography.headlineSmall.copyWith(color: colors.text.accent),
      ),
    );
    body.add(const SizedBox(height: AppSpacing.xs));
    body.add(
      Text(
        failure == null
            ? firstSync
                  ? 'The first sync of a new vault can take a few minutes. It keeps '
                        'going if you leave this screen.'
                  : syncing
                  ? 'Checking for new activity...'
                  : 'Everything is up to date. Pull down on Home to check again.'
            : failure.explanation,
        style: AppTypography.bodyMedium.copyWith(color: colors.text.secondary),
      ),
    );
    body.add(const SizedBox(height: AppSpacing.sm));
    final endpoint = failure?.endpoint;
    final details = <Widget>[
      if (endpoint != null)
        DetailRow(
          label: endpoint == SyncEndpoint.relay ? 'Server' : 'Zcash server',
          value: _host(
            endpoint == SyncEndpoint.relay
                ? vaultRelay ?? endpoints.relayUrl
                : endpoints.lightwalletdUrl,
          ),
        ),
      if (downSince != null)
        DetailRow(label: 'Down since', value: formatSince(downSince)),
      if (ref.watch(torProvider).enabled)
        DetailRow(label: 'Tor', value: ref.watch(torProvider).statusLabel),
      DetailRow(label: 'Last synced', value: formatLastSuccess(syncedAt)),
      if (vault.balance?.height case final height?)
        DetailRow(label: 'Block height', value: '$height'),
      if (endpoint == SyncEndpoint.relay ||
          failure?.kind == SyncFailureKind.offline)
        DetailRow(
          label: 'Server last reached',
          value: formatLastSuccess(refreshedAt),
        ),
    ];
    body.add(
      MobileSurfaceCard(
        cornerRadius: AppRadii.large,
        child: Column(children: details),
      ),
    );
    if (failure != null) _logDetail(failure);
    if (suggested != null) {
      body.addAll([
        const SizedBox(height: AppSpacing.md),
        MoveSuggestion(
          vaultName: name,
          host: relayHost(suggested),
          onMove: () => _moveTo(suggested),
          onPickAnother: () => context.push('/vault-server'),
        ),
      ]);
    }
    if (_repaired != null) {
      body.addAll([
        const SizedBox(height: AppSpacing.sm),
        Text(
          _repaired!,
          style: AppTypography.bodySmall.copyWith(color: colors.text.secondary),
        ),
      ]);
    }

    final actions = <Widget>[
      AppButton(
        expand: true,
        variant: suggested == null
            ? AppButtonVariant.primary
            : AppButtonVariant.secondary,
        onPressed: _retrying ? null : _retry,
        child: Text(_retrying ? 'Trying...' : 'Try again'),
      ),
      if (failure?.canRestoreRelay ?? false) ...[
        const SizedBox(height: AppSpacing.xs),
        ActionButton(
          label: 'Restore vault on the server',
          busyLabel: 'Restoring...',
          variant: AppButtonVariant.secondary,
          errorFallback: 'Couldn\'t restore the vault on the server.',
          onPressed: _restore,
        ),
      ],
      if (failure?.canFollowRelay ?? false) ...[
        const SizedBox(height: AppSpacing.xs),
        ActionButton(
          label: 'Follow the server instead',
          busyLabel: 'Following...',
          variant: AppButtonVariant.secondary,
          errorFallback: 'Couldn\'t follow the server.',
          onPressed: _follow,
        ),
      ],
      if ((failure?.canMoveRelay ?? false) && suggested == null) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: () => context.push('/vault-server'),
          child: Text(moving ? 'See the move' : 'Move to another server'),
        ),
      ],
      if ((failure?.suggestsSettings ?? false) &&
          endpoint != SyncEndpoint.relay) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.ghost,
          onPressed: () => failure!.kind == SyncFailureKind.torFailed
              ? context.go('/settings')
              : context.push('/zcash-server'),
          child: Text(
            failure?.kind == SyncFailureKind.torFailed
                ? 'Tor settings'
                : 'Zcash server',
          ),
        ),
      ],
    ];

    return ZafeScreen(
      title: 'Status',
      bottom: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: actions,
      ),
      children: body,
    );
  }
}

class MoveSuggestion extends StatelessWidget {
  const MoveSuggestion({
    super.key,
    required this.vaultName,
    required this.host,
    this.onMove,
    this.onPickAnother,
  });

  final String vaultName;
  final String host;
  final Future<void> Function()? onMove;
  final VoidCallback? onPickAnother;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.all(AppSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Move $vaultName to $host?',
            style: AppTypography.bodyLarge.copyWith(
              fontWeight: FontWeight.w600,
              color: colors.text.accent,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Still down after ${kSuggestMoveAfter.inHours} hours. Your funds are '
            'safe, and payments go on once enough members move.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          ActionButton(
            label: 'Move',
            errorFallback: 'Couldn\'t start the move. Try again.',
            onPressed: onMove ?? () async {},
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: onPickAnother,
            child: const Text('Pick another server'),
          ),
        ],
      ),
    );
  }
}

String _host(String url) => Uri.tryParse(url)?.authority ?? url;

final _loggedDetails = <String>{};

void _logDetail(SyncFailure failure) {
  if (failure.detail.isEmpty) return;
  final key = '${failure.kind.name}: ${failure.detail}';
  if (!_loggedDetails.add(key)) return;
  unawaited(
    CrashLog.instance?.record('sync-status', key) ?? Future<void>.value(),
  );
}

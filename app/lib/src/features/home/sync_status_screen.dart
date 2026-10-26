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

/// The failure Home shows for the active vault, if any.
SyncFailure? currentSyncFailure(WidgetRef ref) {
  final syncError = ref.watch(vaultProvider.select((v) => v.syncError));
  final relayError = ref.watch(proposalsProvider.select((p) => p.error));
  return homeSyncFailure(syncError: syncError, relayError: relayError);
}

/// Since when this phone can't reach vault `vaultId`'s relay (`vault_relay_status`,
/// written by every relay request), or null.
DateTime? relayUnreachableSince(String vaultId) {
  try {
    return fromUnixSeconds(
      rust_vault.vaultRelayStatus(vaultId: vaultId).unreachableSince,
    );
  } catch (_) {
    return null;
  }
}


/// How long the vault's server must be down before the Status screen suggests moving to
/// a listed one.
const kSuggestMoveAfter = Duration(hours: 6);

/// The first listed server other than `current` that answers and can hold the vault,
/// for "Move Grants to relay2.zafe.cash?" (only the listed ones are asked, one by one).
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

/// Home's status line opens this: whether the vault is in sync, or why not (the reason,
/// the server, when it last worked) and what can fix it: Try again, restore or follow
/// the server's history, and after [kSuggestMoveAfter] of a server outage, a move to a
/// listed server the app picked. A pushed screen, not a sheet.
class SyncStatusScreen extends ConsumerStatefulWidget {
  const SyncStatusScreen({super.key, this.retry});

  /// Runs a refresh and a sync (Home's); null: the proposals and the wallet directly.
  final Future<void> Function()? retry;

  @override
  ConsumerState<SyncStatusScreen> createState() => _SyncStatusScreenState();
}

class _SyncStatusScreenState extends ConsumerState<SyncStatusScreen> {
  bool _retrying = false;

  /// Last sync saved by an earlier session or a background check.
  DateTime? _savedSyncedAt;

  /// The server the app suggests moving to during a long outage (asked once).
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

  /// What the last repair did (shown under the buttons).
  String? _repaired;

  /// Puts the vault's history back on the server from this phone's copy.
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

  /// Drops this phone's copy of the history and follows the server's, after asking.
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
      // A failed Tor must connect before anything else can.
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

  /// Moves to the suggested server: the unlock, then the Server screen's waiting room
  /// (the old server is down, so Nudge is how the others hear of it).
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

    // No error is not the same as "up to date": a sync may be running, and the first one of
    // a new vault takes a while (no balance yet).
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
          // The vault's server is the whole group's, the Zcash server this phone's.
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
    // The technical detail goes to the diagnostic report, never on screen.
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
      // Docs/relay-move.md §5: a way off a server that is down or can't be trusted.
      if ((failure?.canMoveRelay ?? false) && suggested == null) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: () => context.push('/vault-server'),
          child: Text(moving ? 'See the move' : 'Move to another server'),
        ),
      ],
      // The vault's server has its own button above (Move); this is for the Zcash server
      // and Tor.
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

/// A long server outage: the app picked a listed server that answers. Plain data, so it
/// renders without Rust.
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

/// Failures already written to the diagnostic report this session (one line each, so
/// a poll that keeps failing doesn't fill the log).
final _loggedDetails = <String>{};

void _logDetail(SyncFailure failure) {
  if (failure.detail.isEmpty) return;
  final key = '${failure.kind.name}: ${failure.detail}';
  if (!_loggedDetails.add(key)) return;
  unawaited(
    CrashLog.instance?.record('sync-status', key) ?? Future<void>.value(),
  );
}

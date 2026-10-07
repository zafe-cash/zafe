import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/errors/sync_failure.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_provider.dart';

/// The failure Home shows for the active vault, if any.
SyncFailure? currentSyncFailure(WidgetRef ref) {
  final syncError = ref.watch(vaultProvider.select((v) => v.syncError));
  final relayError = ref.watch(proposalsProvider.select((p) => p.error));
  return homeSyncFailure(syncError: syncError, relayError: relayError);
}

/// Why syncing fails: the reason, the server, when it last worked, and "Try again".
Future<void> showSyncStatusSheet(
  BuildContext context, {
  required Future<void> Function() retry,
}) => showAppMobileSheet<void>(
  context: context,
  builder: (_) => _SyncStatusSheet(retry: retry),
);

class _SyncStatusSheet extends ConsumerStatefulWidget {
  const _SyncStatusSheet({required this.retry});
  final Future<void> Function() retry;

  @override
  ConsumerState<_SyncStatusSheet> createState() => _SyncStatusSheetState();
}

class _SyncStatusSheetState extends ConsumerState<_SyncStatusSheet> {
  bool _retrying = false;

  /// Last sync saved by an earlier session or a background check.
  DateTime? _savedSyncedAt;

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

  bool _restoring = false;

  /// Puts the vault's history back on the relay from this phone's copy.
  Future<void> _restore() async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to restore the vault on the relay',
    )) {
      return;
    }
    setState(() => _restoring = true);
    try {
      final entries = await ref.read(proposalsProvider.notifier).restoreRelay();
      if (!mounted) return;
      showAppToast(
        context,
        entries == 0
            ? 'The relay already has everything this phone has.'
            : 'Restored $entries entries on the relay.',
      );
    } catch (e) {
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(e, fallback: 'Couldn\'t restore the relay.'),
          iconName: AppIcons.warningCircle,
          tone: AppToastTone.destructive,
        );
      }
    } finally {
      if (mounted) setState(() => _restoring = false);
    }
  }

  bool _following = false;

  /// Drops this phone's copy of the history and follows the relay's, after asking.
  Future<void> _follow() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: const Text(followRelayTitle),
        content: const Text(followRelayWarning),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(c, true),
            child: const Text('Follow the relay'),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to follow the relay\'s history',
    )) {
      return;
    }
    setState(() => _following = true);
    try {
      final dropped = await ref.read(proposalsProvider.notifier).followRelay();
      if (!mounted) return;
      showAppToast(context, followRelayDone(dropped));
    } catch (e) {
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(e, fallback: 'Couldn\'t follow the relay.'),
          iconName: AppIcons.warningCircle,
          tone: AppToastTone.destructive,
        );
      }
    } finally {
      if (mounted) setState(() => _following = false);
    }
  }

  Future<void> _retry() async {
    setState(() => _retrying = true);
    try {
      // A failed Tor must connect before anything else can.
      final tor = ref.read(torProvider);
      if (tor.failed) await ref.read(torProvider.notifier).retry();
      await widget.retry();
    } finally {
      if (mounted) setState(() => _retrying = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final failure = currentSyncFailure(ref);
    final vault = ref.watch(vaultProvider);
    final endpoints = ref.watch(endpointsProvider);
    final refreshedAt = ref.watch(
      proposalsProvider.select((p) => p.refreshedAt),
    );
    final syncedAt =
        (vault.activeId == null ? null : vault.syncedAt[vault.activeId]) ??
        _savedSyncedAt;

    final body = <Widget>[];
    if (failure == null) {
      body.add(
        Text(
          _retrying ? 'Trying again...' : 'Everything is up to date.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
      );
    } else {
      body.add(
        Text(
          failure.explanation,
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
      );
    }
    body.add(const SizedBox(height: AppSpacing.sm));
    final endpoint = failure?.endpoint;
    if (endpoint != null) {
      body.add(
        DetailRow(
          label: endpoint == SyncEndpoint.relay ? 'Relay' : 'Zcash server',
          value: _host(
            endpoint == SyncEndpoint.relay
                ? endpoints.relayUrl
                : endpoints.lightwalletdUrl,
          ),
        ),
      );
    }
    final tor = ref.watch(torProvider);
    if (tor.enabled) {
      body.add(DetailRow(label: 'Tor', value: tor.statusLabel));
    }
    body.add(
      DetailRow(label: 'Last synced', value: formatLastSuccess(syncedAt)),
    );
    final height = vault.balance?.height;
    if (height != null) {
      body.add(DetailRow(label: 'Block height', value: '$height'));
    }
    if (endpoint == SyncEndpoint.relay ||
        failure?.kind == SyncFailureKind.offline) {
      body.add(
        DetailRow(
          label: 'Relay last reached',
          value: formatLastSuccess(refreshedAt),
        ),
      );
    }
    if (failure != null && failure.detail.isNotEmpty) {
      body.addAll([
        const SizedBox(height: AppSpacing.sm),
        SelectableText(
          failure.detail,
          maxLines: 4,
          style: AppTypography.codeSmall.copyWith(color: colors.text.secondary),
        ),
      ]);
    }
    body.addAll([
      const SizedBox(height: AppSpacing.md),
      AppButton(
        expand: true,
        onPressed: _retrying ? null : _retry,
        child: Text(_retrying ? 'Trying...' : 'Try again'),
      ),
      if (failure?.canRestoreRelay ?? false) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: _restoring ? null : _restore,
          child: Text(
            _restoring ? 'Restoring...' : 'Restore vault on the relay',
          ),
        ),
      ],
      if (failure?.canFollowRelay ?? false) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: _following ? null : _follow,
          child: Text(_following ? 'Following...' : 'Follow the relay instead'),
        ),
      ],
      if (failure?.suggestsSettings ?? false) ...[
        const SizedBox(height: AppSpacing.xs),
        AppButton(
          expand: true,
          variant: AppButtonVariant.ghost,
          onPressed: () {
            final router = GoRouter.of(context);
            Navigator.of(context).pop();
            router.go('/settings');
          },
          child: Text(
            failure?.kind == SyncFailureKind.torFailed
                ? 'Tor settings'
                : 'Server settings',
          ),
        ),
      ],
    ]);

    return MobileModalScaffold(
      title: failure?.title ?? 'Vault in sync',
      titleMaxLines: 2,
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: body,
      ),
    );
  }
}

String _host(String url) => Uri.tryParse(url)?.authority ?? url;

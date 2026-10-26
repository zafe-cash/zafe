import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_confirm.dart';
import '../../core/widgets/app_menu.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/vault.dart' as rust;
import '../home/rename_sheet.dart';

/// One vault's ⋯ on Manage vaults: a menu of rename, back up, viewing key, how it's
/// protected and remove. The ones that work on the open vault switch to it first.
class VaultActionsMenu extends ConsumerWidget {
  const VaultActionsMenu({super.key, required this.vaultId});
  final String vaultId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final vault = ref
        .watch(vaultProvider)
        .vaults
        .where((v) => v.id == vaultId)
        .firstOrNull;
    if (vault == null) return const SizedBox.shrink();
    final ready = vault.ready;
    final summary = ready ? rust.vaultSummary(material: vault.material!) : null;
    final name =
        ref.watch(vaultNamesProvider)[vaultId] ??
        summary?.name ??
        (vault.invite == null
            ? 'Vault'
            : rust.parseInvite(invite: vault.invite!).name);

    Future<void> onVault(Future<void> Function() then) async {
      if (ref.read(vaultProvider).activeId != vaultId) {
        await ref.read(vaultProvider.notifier).switchTo(vaultId);
      }
      if (context.mounted) await then();
    }

    return AppMenu(
      items: [
        if (ready) ...[
          AppMenuItem(
            icon: AppIcons.edit,
            label: 'Rename',
            onTap: () => onVault(() => showRenameVaultSheet(context, ref)),
          ),
          AppMenuItem(
            icon: AppIcons.lock,
            label: 'Back up',
            onTap: () => onVault(() async => context.push('/export')),
          ),
          AppMenuItem(
            icon: AppIcons.eye,
            label: 'Viewing key',
            onTap: () => onVault(() => openViewingKey(context, ref)),
          ),
          AppMenuItem(
            icon: AppIcons.shieldKeyhole,
            label: 'How it\'s protected',
            onTap: () =>
                onVault(() async => context.push('/vault-protection')),
          ),
        ],
        AppMenuItem(
          icon: AppIcons.trash,
          label: 'Remove from this phone',
          destructive: true,
          onTap: () => confirmRemoveVault(
            context,
            ref,
            vaultId: vaultId,
            name: name,
            threshold: summary?.threshold,
          ),
        ),
      ],
      builder: (context, open) => AppTappable(
        semanticsLabel: 'More',
        onTap: open,
        child: SizedBox(
          width: 44,
          height: 44,
          child: Center(
            child: AppIcon(AppIcons.more, size: 20, color: colors.icon.muted),
          ),
        ),
      ),
    );
  }
}

/// The viewing key reveals the vault's whole history: an unlock first.
Future<void> openViewingKey(BuildContext context, WidgetRef ref) async {
  if (!await confirmUnlock(context, ref, reason: 'Show the viewing key')) {
    return;
  }
  if (context.mounted) unawaited(context.push('/viewing-key'));
}

/// Removing a vault deletes this member's key share from the phone: the one action
/// here that can't be undone, so it asks, then wants an unlock.
Future<void> confirmRemoveVault(
  BuildContext context,
  WidgetRef ref, {
  required String vaultId,
  required String name,
  int? threshold,
}) async {
  final remove = await showAppConfirm(
    context,
    title: 'Remove "$name"?',
    body: threshold == null
        ? 'This phone leaves the setup.'
        : 'This deletes your key share from this phone. If fewer than '
              '$threshold members keep theirs, the funds are lost for good.',
    confirmLabel: 'Remove from this phone',
    cancelLabel: 'Keep it',
  );
  if (!remove || !context.mounted) return;
  if (!await confirmUnlock(context, ref, reason: 'Remove this vault')) return;
  await ref.read(vaultProvider.notifier).removeVault(vaultId);
  if (!context.mounted) return;
  final next = ref.read(vaultProvider);
  context.go(
    next.hasVault
        ? '/home'
        : next.isSettingUp
        ? '/setup'
        : '/welcome',
  );
}

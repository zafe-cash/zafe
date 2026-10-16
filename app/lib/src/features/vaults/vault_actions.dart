import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/vault.dart' as rust;
import '../home/rename_sheet.dart';

/// One vault's actions (from its ⋯ on Manage vaults): rename, back up, viewing key,
/// how it's protected, remove. The ones that work on the open vault switch to it first.
Future<void> showVaultActions(
  BuildContext context,
  WidgetRef ref,
  String vaultId,
) async {
  final vault = ref
      .read(vaultProvider)
      .vaults
      .where((v) => v.id == vaultId)
      .firstOrNull;
  if (vault == null) return;
  final ready = vault.ready;
  final summary = ready ? rust.vaultSummary(material: vault.material!) : null;
  final name =
      ref.read(vaultNamesProvider)[vaultId] ??
      summary?.name ??
      (vault.invite == null
          ? 'Vault'
          : rust.parseInvite(invite: vault.invite!).name);

  Future<void> onVault(Future<void> Function() then) async {
    Navigator.of(context).pop();
    if (ref.read(vaultProvider).activeId != vaultId) {
      await ref.read(vaultProvider.notifier).switchTo(vaultId);
    }
    if (context.mounted) await then();
  }

  await showAppMobileSheet<void>(
    context: context,
    builder: (sheet) {
      final colors = sheet.colors;
      Widget item(
        String icon,
        String label,
        VoidCallback onTap, {
        bool destructive = false,
      }) => AppTappable(
        onTap: onTap,
        semanticsLabel: label,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: AppSpacing.s),
          child: Row(
            children: [
              AppIcon(
                icon,
                size: 20,
                color: destructive
                    ? colors.icon.destructive
                    : colors.icon.muted,
                patina: destructive ? colors.icon.destructive : null,
              ),
              const SizedBox(width: AppSpacing.s),
              Text(
                label,
                style: AppTypography.labelLarge.copyWith(
                  color: destructive
                      ? colors.text.destructive
                      : colors.text.accent,
                ),
              ),
            ],
          ),
        ),
      );
      return MobileModalScaffold(
        title: name,
        onClose: () => Navigator.of(sheet).pop(),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (ready) ...[
              item(
                AppIcons.edit,
                'Rename',
                () => onVault(() => showRenameVaultSheet(context, ref)),
              ),
              item(
                AppIcons.lock,
                'Back up',
                () => onVault(() async => context.push('/export')),
              ),
              item(
                AppIcons.eye,
                'Viewing key',
                () => onVault(() async {
                  if (!await confirmUnlock(
                    context,
                    ref,
                    reason: 'Show the viewing key',
                  )) {
                    return;
                  }
                  if (context.mounted) unawaited(context.push('/viewing-key'));
                }),
              ),
              item(
                AppIcons.shieldKeyhole,
                'How it\'s protected',
                () => onVault(() async => context.push('/vault-protection')),
              ),
            ],
            item(AppIcons.trash, 'Remove from this phone', () {
              Navigator.of(sheet).pop();
              unawaited(
                confirmRemoveVault(
                  context,
                  ref,
                  vaultId: vaultId,
                  name: name,
                  threshold: summary?.threshold,
                ),
              );
            }, destructive: true),
          ],
        ),
      );
    },
  );
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
  final remove = await showAppMobileSheet<bool>(
    context: context,
    builder: (sheet) {
      final colors = sheet.colors;
      return MobileModalScaffold(
        title: 'Remove "$name"?',
        onClose: () => Navigator.of(sheet).pop(false),
        leading: AppIcon(
          AppIcons.warning,
          size: 20,
          color: colors.icon.destructive,
          patina: colors.icon.destructive,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              threshold == null
                  ? 'This phone leaves the setup.'
                  : 'This deletes your key share from this phone. If fewer than '
                        '$threshold members keep theirs, the funds are lost for good.',
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.accent,
              ),
            ),
            const SizedBox(height: AppSpacing.md),
            AppButton(
              expand: true,
              variant: AppButtonVariant.destructive,
              onPressed: () => Navigator.of(sheet).pop(true),
              child: const Text('Remove from this phone'),
            ),
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: () => Navigator.of(sheet).pop(false),
              child: const Text('Keep it'),
            ),
          ],
        ),
      );
    },
  );
  if (remove != true || !context.mounted) return;
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

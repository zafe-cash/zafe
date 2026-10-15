import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/confirm_option.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../onboarding/onboarding_art.dart';

/// Whether the active vault has a backup (drives the home reminder).
final backupStatusProvider = FutureProvider.autoDispose<bool>((ref) async {
  final id = ref.watch(vaultProvider.select((v) => v.activeId));
  if (id == null) return true;
  return (await VaultSummaries.read(id)).backedUp;
});

/// Shown right after a vault's keys are created: the member's key share now exists only on
/// this phone, so offer to back it up before anything else.
class BackupPromptScreen extends ConsumerWidget {
  const BackupPromptScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final summary = ref.watch(vaultProvider).summary;
    final name = ref.watch(activeVaultNameProvider) ?? 'your vault';
    final t = summary?.threshold ?? 0;
    final n = summary?.members.length ?? 0;
    return ZafeScreen(
      title: 'Back up vault',
      showBack: false,
      bottom: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.lock, size: 20),
            onPressed: () => context.push('/export'),
            child: const Text('Back up now'),
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () async {
              final choice = await confirmSkipBackup(context, t: t, n: n);
              if (!context.mounted) return;
              switch (choice) {
                case SkipBackupChoice.skip:
                  context.go('/home');
                case SkipBackupChoice.backUp:
                  unawaited(context.push('/export'));
                case null:
                  break;
              }
            },
            child: const Text('Later'),
          ),
        ],
      ),
      children: [
        const OnboardingBanner('backup_seal'),
        const SizedBox(height: AppSpacing.md),
        Text(
          '"$name" is ready',
          style: TextStyle(
            fontFamily: 'Space Grotesk',
            fontSize: 28,
            height: 1.2,
            color: colors.text.accent,
          ),
        ),
        const SizedBox(height: AppSpacing.s),
        Text(
          'Your key share for this vault now exists only on this phone. If you lose the phone '
          'without a backup, your seat is gone, and '
          '${n == t ? 'if any member loses theirs' : 'if more than ${n - t} of the $n members lose theirs'}, '
          'the vault\'s funds can never be moved.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (final (icon, text) in const [
                (AppIcons.lock, 'Encrypted with a passphrase only you know'),
                (
                  AppIcons.share,
                  'Save the file anywhere: cloud drive, password manager, USB',
                ),
                (AppIcons.key, 'Restores your seat on a new phone'),
              ])
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
                  child: Row(
                    children: [
                      AppIcon(icon, size: 20, color: colors.icon.muted),
                      const SizedBox(width: AppSpacing.s),
                      Expanded(
                        child: Text(
                          text,
                          style: AppTypography.bodySmall.copyWith(
                            color: colors.text.primary,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

/// What skipping the backup risks, for the whole group: with t of n, the vault survives
/// only n - t lost phones without backups.
String skipBackupRisk({required int t, required int n}) {
  final spare = n - t;
  return switch (spare) {
    <= 0 =>
      'If I lose this phone without a backup, the vault\'s funds can never be moved '
          'again: every member is needed.',
    1 =>
      'If I lose this phone without a backup, the vault keeps working only while no '
          'other member loses theirs.',
    _ =>
      'If I lose this phone without a backup, the vault keeps working only while no '
          'more than ${spare - 1} other ${spare == 2 ? 'member loses' : 'members lose'} '
          'theirs.',
  };
}

enum SkipBackupChoice { skip, backUp }

/// "Later" on the backup prompt: the member acknowledges what skipping risks for the
/// group before going on. `null` when the sheet was closed.
Future<SkipBackupChoice?> confirmSkipBackup(
  BuildContext context, {
  required int t,
  required int n,
}) => showAppMobileSheet<SkipBackupChoice>(
  context: context,
  builder: (sheet) => SkipBackupSheet(
    risk: skipBackupRisk(t: t, n: n),
  ),
);

class SkipBackupSheet extends StatefulWidget {
  const SkipBackupSheet({super.key, required this.risk});
  final String risk;

  @override
  State<SkipBackupSheet> createState() => _SkipBackupSheetState();
}

class _SkipBackupSheetState extends State<SkipBackupSheet> {
  bool _understood = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileModalScaffold(
      title: 'Back up later?',
      onClose: () => Navigator.of(context).pop(),
      constrainBody: true,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Home keeps reminding you until you do.',
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.md),
            ConfirmOption(
              selected: _understood,
              label: widget.risk,
              onTap: () => setState(() => _understood = !_understood),
            ),
            const SizedBox(height: AppSpacing.md),
            AppButton(
              expand: true,
              onPressed: () =>
                  Navigator.of(context).pop(SkipBackupChoice.backUp),
              child: const Text('Back up now'),
            ),
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _understood
                  ? () => Navigator.of(context).pop(SkipBackupChoice.skip)
                  : null,
              child: const Text('Back up later'),
            ),
          ],
        ),
      ),
    );
  }
}

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
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
    final name = ref.watch(activeVaultNameProvider) ?? 'your vault';
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
            // No second question: Home keeps reminding until it's done.
            onPressed: () => context.go('/home'),
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
          'Your key share exists only on this phone. Lose it without a backup and your '
          'seat is gone.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
      ],
    );
  }
}

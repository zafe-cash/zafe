import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/config/beta.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_copy_feedback.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/dot_qr_shape.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';

class ReceiveScreen extends ConsumerWidget {
  const ReceiveScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final summary = ref.watch(vaultProvider).summary;
    if (summary == null) return const SizedBox.shrink();
    final address = summary.address;
    final compact =
        '${address.substring(0, 13)} ... ${address.substring(address.length - 11)}';

    return ZafeScreen(
      title: 'Receive',
      children: [
        const SizedBox(height: AppSpacing.md),
        Center(
          child: Container(
            width: 292,
            height: 308,
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 24),
            decoration: BoxDecoration(
              color: colors.background.darkCard,
              borderRadius: BorderRadius.circular(AppRadii.xLarge),
              border: Border.all(color: colors.border.subtleOpacity),
            ),
            child: PrettyQrView.data(
              data: address,
              decoration: PrettyQrDecoration(
                shape: DotQrShape(color: colors.text.darkCard),
              ),
            ),
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Center(
          child: Text(
            ref.watch(activeVaultNameProvider) ?? summary.name,
            style: AppTypography.bodyLarge.copyWith(
              color: colors.text.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
        const SizedBox(height: AppSpacing.xxs),
        Center(
          child: Text(
            compact,
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        Center(
          child: Text(
            'Shielded address. Funds sent here are held by the vault and can only move with '
            '${summary.threshold} of ${summary.members.length} signatures.',
            textAlign: TextAlign.center,
            style: AppTypography.bodySmall.copyWith(color: colors.text.muted),
          ),
        ),
        if (kIsBeta) ...[
          const SizedBox(height: AppSpacing.xs),
          Center(
            child: Text(
              betaNote(),
              textAlign: TextAlign.center,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.warning,
              ),
            ),
          ),
        ],
        const SizedBox(height: AppSpacing.base),
        AppButton(
          expand: true,
          onPressed: () => SharePlus.instance.share(ShareParams(text: address)),
          leading: const AppIcon(AppIcons.share, size: 20),
          child: const Text('Share vault address'),
        ),
        const SizedBox(height: AppSpacing.s),
        AppButton(
          expand: true,
          variant: AppButtonVariant.ghost,
          onPressed: () => copyTextWithToast(
            context,
            text: address,
            toastMessage: 'Address copied',
          ),
          leading: const AppIcon(AppIcons.copy, size: 20),
          child: const Text('Copy vault address'),
        ),
      ],
    );
  }
}

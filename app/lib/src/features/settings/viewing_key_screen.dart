import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_copy_feedback.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/dot_qr_shape.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/vault.dart' as rust;

/// The vault's viewing key (UFVK) for an auditor or accountant: copy or QR. Opened from
/// Settings after an unlock; the route blocks screenshots (SecureScreen).
class ViewingKeyScreen extends ConsumerWidget {
  const ViewingKeyScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final material = vault.material;
    if (material == null) return const SizedBox.shrink();
    String? key;
    String? error;
    try {
      key = rust.vaultViewingKey(material: material);
    } catch (e) {
      error = zafeErrorMessage(e, fallback: 'Couldn\'t read the viewing key.');
    }

    Widget note(String text, {Color? color}) => Text(
      text,
      style: AppTypography.bodySmall.copyWith(
        color: color ?? colors.text.secondary,
      ),
    );

    return ZafeScreen(
      title: 'Viewing key',
      bottom: key == null
          ? null
          : AppButton(
              expand: true,
              leading: const AppIcon(AppIcons.copy, size: 20),
              onPressed: () => copyTextWithToast(
                context,
                text: key!,
                toastMessage: 'Viewing key copied',
                sensitive: true,
              ),
              child: const Text('Copy viewing key'),
            ),
      children: [
        Container(
          padding: const EdgeInsets.all(AppSpacing.sm),
          decoration: BoxDecoration(
            color: colors.background.neutralSubtleOpacity,
            borderRadius: BorderRadius.circular(AppRadii.large),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              AppIcon(
                AppIcons.warningCircle,
                size: 20,
                color: colors.icon.accent,
              ),
              const SizedBox(width: AppSpacing.xs),
              Expanded(
                child: note(
                  'Anyone with this key sees every past and future payment of '
                  '"${ref.watch(activeVaultNameProvider) ?? 'this vault'}": amounts, memos and '
                  'balance. It can\'t spend. Share it only with someone you trust '
                  'with the vault\'s history, like an auditor or accountant. It '
                  'can\'t be revoked.',
                  color: colors.text.accent,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        if (error != null)
          note(error, color: colors.text.destructive)
        else ...[
          Center(
            child: Container(
              width: 292,
              height: 292,
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: colors.background.darkCard,
                borderRadius: BorderRadius.circular(AppRadii.xLarge),
                border: Border.all(color: colors.border.subtleOpacity),
              ),
              child: PrettyQrView.data(
                data: key!,
                decoration: PrettyQrDecoration(
                  shape: DotQrShape(color: colors.text.darkCard),
                ),
              ),
            ),
          ),
          const SizedBox(height: AppSpacing.md),
          SelectableText(
            key,
            style: AppTypography.codeSmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          note(
            'Wallets that import a unified full viewing key can show this vault\'s '
            'history read-only.',
            color: colors.text.muted,
          ),
        ],
      ],
    );
  }
}

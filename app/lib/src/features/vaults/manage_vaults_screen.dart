import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import 'vault_actions.dart';
import 'vault_switcher_sheet.dart';

/// Every vault on this phone: the open one, then the others; each row's ⋯ holds that
/// vault's actions (rename, back up, viewing key, how it's protected, remove).
class ManageVaultsScreen extends ConsumerWidget {
  const ManageVaultsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final state = ref.watch(vaultProvider);
    final names = ref.watch(vaultNamesProvider);
    final current = state.vaults.where((v) => v.id == state.activeId).toList();
    final others = state.vaults.where((v) => v.id != state.activeId).toList();
    Widget group(String title, List<Widget> rows) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: AppSpacing.xxs),
          child: Text(
            title,
            style: AppTypography.labelMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          padding: const EdgeInsets.symmetric(
            horizontal: AppSpacing.s,
            vertical: AppSpacing.xs,
          ),
          child: Column(children: rows),
        ),
        const SizedBox(height: AppSpacing.md),
      ],
    );
    Widget row(v) => VaultRow(
      vault: v,
      localName: names[v.id],
      showRule: true,
      onTap: () async {
        if (v.id != state.activeId) {
          await ref.read(vaultProvider.notifier).switchTo(v.id);
        }
        if (context.mounted) context.go(v.ready ? '/home' : '/setup');
      },
      trailing: AppTappable(
        semanticsLabel: 'More',
        onTap: () => showVaultActions(context, ref, v.id),
        child: SizedBox(
          width: 44,
          height: 44,
          child: Center(
            child: AppIcon(AppIcons.more, size: 20, color: colors.icon.muted),
          ),
        ),
      ),
    );
    return ZafeScreen(
      title: 'Vaults',
      bottom: AppButton(
        expand: true,
        variant: AppButtonVariant.secondary,
        leading: const AppIcon(AppIcons.plus, size: 20),
        onPressed: () {
          ref.read(vaultProvider.notifier).beginAddVault();
          context.go('/welcome');
        },
        child: const Text('Add vault'),
      ),
      children: [
        if (current.isNotEmpty)
          group('Current', [for (final v in current) row(v)]),
        if (others.isNotEmpty) group('Other', [for (final v in others) row(v)]),
      ],
    );
  }
}

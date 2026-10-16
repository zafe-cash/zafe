import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/formatting/zec_amount.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/storage/zafe_secure_store.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/vault.dart' as rust;
import 'vault_emblem.dart';

/// Tapping the vault on Home: the open vault in large, the others to switch to (a dot
/// when one has something waiting for you), then Manage vaults and "+" to add one.
Future<void> showVaultSwitcher(BuildContext context) =>
    showAppMobileSheet<void>(
      context: context,
      builder: (_) => const _VaultSwitcher(),
    );

class _VaultSwitcher extends ConsumerWidget {
  const _VaultSwitcher();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final state = ref.watch(vaultProvider);
    final names = ref.watch(vaultNamesProvider);
    final current = state.vaults
        .where((v) => v.id == state.activeId)
        .firstOrNull;
    final others = state.vaults.where((v) => v.id != state.activeId).toList();
    return MobileModalScaffold(
      title: '',
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (current != null) ...[
            Center(child: VaultEmblem(vaultId: current.id, size: 56)),
            const SizedBox(height: AppSpacing.s),
            Text(
              vaultDisplayName(current, names[current.id]),
              textAlign: TextAlign.center,
              style: AppTypography.headlineSmall.copyWith(
                color: colors.text.accent,
              ),
            ),
            if (state.balances[current.id] case final b?) ...[
              const SizedBox(height: AppSpacing.xxs),
              Text(
                amountWithTicker(
                  ZecAmount.fromZatoshi(b.totalZat).balance.amountText,
                  hide: ref.watch(privacyModeProvider),
                ),
                textAlign: TextAlign.center,
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.secondary,
                ),
              ),
            ],
            const SizedBox(height: AppSpacing.md),
          ],
          if (others.isNotEmpty) ...[
            Text(
              'Other vaults',
              style: AppTypography.labelMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.xs),
            ConstrainedBox(
              // About four rows; more scroll.
              constraints: const BoxConstraints(maxHeight: 248),
              child: SingleChildScrollView(
                child: Column(
                  children: [
                    for (final v in others)
                      VaultRow(
                        vault: v,
                        localName: names[v.id],
                        onTap: () async {
                          Navigator.of(context).pop();
                          await ref.read(vaultProvider.notifier).switchTo(v.id);
                          if (context.mounted) {
                            context.go(v.ready ? '/home' : '/setup');
                          }
                        },
                      ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: AppSpacing.md),
          ],
          Row(
            children: [
              Expanded(
                child: AppButton(
                  expand: true,
                  variant: AppButtonVariant.secondary,
                  onPressed: () {
                    Navigator.of(context).pop();
                    context.push('/vaults');
                  },
                  child: const Text('Manage vaults'),
                ),
              ),
              const SizedBox(width: AppSpacing.xs),
              SizedBox(
                width: 80,
                child: AppButton(
                  expand: true,
                  onPressed: () {
                    Navigator.of(context).pop();
                    ref.read(vaultProvider.notifier).beginAddVault();
                    context.go('/welcome');
                  },
                  child: Semantics(
                    label: 'Add vault',
                    child: const AppIcon(AppIcons.plus, size: 22),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// The name this phone shows for a vault: its local rename, the creator's name, or
/// the invite's while it's being set up.
String vaultDisplayName(StoredVault v, String? localName) {
  if (localName != null && localName.isNotEmpty) return localName;
  if (v.ready) return rust.vaultSummary(material: v.material!).name;
  if (v.invite != null) return rust.parseInvite(invite: v.invite!).name;
  return 'Vault';
}

/// A vault in a list: emblem, name, balance (or "Setting up"), a dot when payments wait
/// for you there, and an optional trailing widget (the ⋯ on Manage vaults).
class VaultRow extends ConsumerWidget {
  const VaultRow({
    super.key,
    required this.vault,
    required this.onTap,
    this.localName,
    this.showRule = false,
    this.trailing,
  });

  final StoredVault vault;
  final String? localName;
  final VoidCallback onTap;

  /// Adds the approval rule and network to the detail line.
  final bool showRule;
  final Widget? trailing;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final hide = ref.watch(privacyModeProvider);
    final live = ref.watch(vaultProvider).balances[vault.id]?.totalZat;
    final summary = vault.ready
        ? rust.vaultSummary(material: vault.material!)
        : null;
    final name = vaultDisplayName(vault, localName);
    return AppTappable(
      onTap: onTap,
      semanticsLabel: name,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
        child: FutureBuilder<VaultSummaryInfo>(
          future: VaultSummaries.read(vault.id),
          builder: (context, snap) {
            final info = snap.data;
            final balance = live ?? info?.balanceZat;
            final parts = [
              if (!vault.ready)
                'Setting up'
              else if (balance != null)
                amountWithTicker(
                  ZecAmount.fromZatoshi(balance).balance.amountText,
                  hide: hide,
                ),
              if (showRule && summary != null)
                '${summary.threshold} of ${summary.members.length} signers',
            ];
            final waiting = (info?.actionable ?? 0) > 0;
            return Row(
              children: [
                Stack(
                  clipBehavior: Clip.none,
                  children: [
                    VaultEmblem(vaultId: vault.id),
                    // Something waits for you in this vault.
                    if (waiting)
                      Positioned(
                        right: -2,
                        top: -2,
                        child: Container(
                          width: 12,
                          height: 12,
                          decoration: BoxDecoration(
                            color: colors.background.brandStrong,
                            shape: BoxShape.circle,
                            border: Border.all(
                              color: colors.background.ground,
                              width: 2,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
                const SizedBox(width: AppSpacing.s),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.labelLarge.copyWith(
                          color: colors.text.accent,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      if (parts.isNotEmpty) ...[
                        const SizedBox(height: 2),
                        Text(
                          parts.join(' · '),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AppTypography.labelMedium.copyWith(
                            color: colors.text.secondary,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                ?trailing,
              ],
            );
          },
        ),
      ),
    );
  }
}

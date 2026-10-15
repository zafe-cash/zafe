import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/layout/mobile/mobile_top_scroll_fade.dart';
import '../../core/formatting/member_label.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../home/rename_sheet.dart';

/// The vault's signers and its approval rule (a tab). Signer management, such as
/// rotating a key, will live here.
class SignersScreen extends ConsumerWidget {
  const SignersScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final summary = vault.summary;
    if (summary == null) return const SizedBox.shrink();
    final names = ref.watch(signerNamesProvider);
    final me = vault.myKeyHex;
    final threshold = summary.threshold;
    final members = summary.members;
    final proposals = ref.watch(proposalsProvider);
    // Unknown until the vault log has been read.
    final backedUp = proposals.loaded ? proposals.backedUp : null;

    return SafeArea(
      bottom: false,
      child: Column(
        children: [
          const MobileTopNav.back(title: 'Signers'),
          Expanded(
            child: MobileTopScrollFade(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 112),
                children: [
                  MobileSurfaceCard(
                    cornerRadius: AppRadii.large,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '$threshold of ${members.length} to send',
                          style: AppTypography.headlineSmall.copyWith(
                            color: colors.text.accent,
                          ),
                        ),
                        const SizedBox(height: AppSpacing.xxs),
                        Text(
                          threshold == 1
                              ? 'Any one signer can send a payment on their own.'
                              : 'Every payment needs approvals from $threshold '
                                    'of these signers before it can be sent.',
                          style: AppTypography.bodyMedium.copyWith(
                            color: colors.text.secondary,
                          ),
                        ),
                      ],
                    ),
                  ),
                  for (final move in proposals.seatMoves) ...[
                    const SizedBox(height: AppSpacing.md),
                    SeatMoveCard(
                      who: memberLabel(move.oldKeyHex, me: me, names: names),
                      approvals: move.approvals.length,
                      needed: move.needed,
                      // The signer who lost the phone can't approve; nor twice.
                      onApprove:
                          move.oldKeyHex == me || move.approvals.contains(me)
                          ? null
                          : () => context.push(
                              '/replace-signer',
                              extra: (move.oldKeyHex, move.code),
                            ),
                    ),
                  ],
                  for (final r in proposals.repairs)
                    if (r.newKeyHex != me) ...[
                      const SizedBox(height: AppSpacing.md),
                      RepairCard(
                        who: memberLabel(r.newKeyHex, me: me, names: names),
                        helpers: [
                          for (final h in r.helpers)
                            (h, memberLabel(h, me: me, names: names)),
                        ],
                        meHelping: r.helpers.contains(me),
                        onTakeOver: (stalled, name) => _takeOver(
                          context,
                          ref,
                          r.replacement,
                          stalled,
                          name,
                        ),
                      ),
                    ],
                  if (backedUp != null) ...[
                    const SizedBox(height: AppSpacing.md),
                    BackupHealthCard(
                      signers: members.length,
                      threshold: threshold,
                      backedUp: members.where(backedUp.contains).length,
                    ),
                  ],
                  const SizedBox(height: AppSpacing.md),
                  MobileSurfaceCard(
                    cornerRadius: AppRadii.large,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        for (final m in members)
                          SignerRow(
                            keyHex: m,
                            me: me,
                            name: names[m],
                            trailing: backedUp == null
                                ? null
                                : BackupLabel(backedUp: backedUp.contains(m)),
                            onTap: m == me
                                ? () => showMyNameSheet(context, ref)
                                : () => showRenameSignerSheet(context, ref, m),
                          ),
                      ],
                    ),
                  ),
                  const SizedBox(height: AppSpacing.s),
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: AppSpacing.xxs,
                    ),
                    child: Text(
                      'Tap a signer to label them on this phone: only you see your '
                      'labels. Members can also name themselves from their own row, '
                      'and everyone sees that name.',
                      style: AppTypography.bodySmall.copyWith(
                        color: colors.text.muted,
                      ),
                    ),
                  ),
                  if (members.length > 1) ...[
                    const SizedBox(height: AppSpacing.md),
                    AppButton(
                      expand: true,
                      variant: AppButtonVariant.secondary,
                      leading: const AppIcon(AppIcons.renew, size: 20),
                      onPressed: () => context.push('/replace-signer'),
                      child: const Text('Replace a lost phone'),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Asks, then takes over from a helper who isn't rebuilding a moved seat's key.
Future<void> _takeOver(
  BuildContext context,
  WidgetRef ref,
  BigInt replacement,
  String stalled,
  String name,
) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (c) => AlertDialog(
      title: Text('Help instead of $name?'),
      content: Text(
        'Your phone takes $name\'s place in rebuilding the key, and the rebuild starts '
        'over. Do this only if $name can\'t open Zafe for a while.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(c, false),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(c, true),
          child: const Text('Help instead'),
        ),
      ],
    ),
  );
  if (ok != true || !context.mounted) return;
  if (!await confirmUnlock(
    context,
    ref,
    reason: 'Unlock to help rebuild a key',
  )) {
    return;
  }
  try {
    await ref
        .read(proposalsProvider.notifier)
        .retryRepair(replacement, stalled);
    if (context.mounted) {
      showAppToast(context, 'Your phone helps rebuild the key now.');
    }
  } catch (e) {
    if (context.mounted) {
      showAppToast(
        context,
        zafeErrorMessage(e, fallback: 'Couldn\'t start over. Try again.'),
        iconName: AppIcons.warningCircle,
        tone: AppToastTone.destructive,
      );
    }
  }
}

/// How many signers told the vault they have a backup, and what it means if they don't
/// (spec §12.1–12.2).
class BackupHealthCard extends StatelessWidget {
  const BackupHealthCard({
    super.key,
    required this.signers,
    required this.threshold,
    required this.backedUp,
  });

  final int signers;
  final int threshold;
  final int backedUp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final missing = signers - backedUp;
    final spare = signers - threshold;
    final ok = missing == 0;
    final String body;
    if (ok) {
      body = 'Every signer has saved a backup of their key.';
    } else if (missing <= spare) {
      body =
          '${missing == 1 ? '1 signer hasn\'t' : '$missing signers haven\'t'} saved '
          'a backup yet. The vault keeps working if up to $spare '
          '${spare == 1 ? 'signer loses their phone' : 'signers lose their phones'}, '
          'but each loss without a backup uses up that margin.';
    } else {
      body =
          '$missing signers haven\'t saved a backup yet. If more than $spare of '
          '${spare == 1 ? 'them loses their phone' : 'them lose their phones'}, '
          'the funds are lost for good.';
    }
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppIcon(
            ok ? AppIcons.checkCircle : AppIcons.warningCircle,
            size: 22,
            color: ok ? colors.icon.accent : colors.icon.warning,
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Backups: $backedUp of $signers',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  body,
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// "Backed up" or "No backup" at the end of a signer row.
class BackupLabel extends StatelessWidget {
  const BackupLabel({super.key, required this.backedUp});

  final bool backedUp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Text(
      backedUp ? 'Backed up' : 'No backup',
      style: AppTypography.bodySmall.copyWith(
        color: backedUp ? colors.text.muted : colors.text.warning,
      ),
    );
  }
}

/// A signer moving to a new phone, waiting for co-signers' approvals.
class SeatMoveCard extends StatelessWidget {
  const SeatMoveCard({
    super.key,
    required this.who,
    required this.approvals,
    required this.needed,
    this.onApprove,
  });

  final String who;
  final int approvals;
  final int needed;

  /// Null when this member can't approve (it's their seat, or they already did).
  final VoidCallback? onApprove;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppIcon(AppIcons.renew, size: 22, color: colors.icon.accent),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  '$who is moving to a new phone',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Approvals: $approvals of $needed. Before approving, you and $who compare '
            'safety words: their new phone shows the words your phone gets for it.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          if (onApprove != null) ...[
            const SizedBox(height: AppSpacing.s),
            SizedBox(
              width: double.infinity,
              child: AppButton(
                expand: true,
                size: AppButtonSize.medium,
                onPressed: onApprove,
                child: const Text('Check and approve'),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A moved seat whose key the helpers' phones are still rebuilding on the new phone. A
/// member who isn't helping can take over from a helper who can't.
class RepairCard extends StatelessWidget {
  const RepairCard({
    super.key,
    required this.who,
    required this.helpers,
    required this.meHelping,
    this.onTakeOver,
  });

  final String who;

  /// `(key hex, label)` of the helpers in the current attempt.
  final List<(String, String)> helpers;
  final bool meHelping;
  final void Function(String stalled, String name)? onTakeOver;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final names = helpers.map((h) => h.$2).join(' and ');
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppIcon(AppIcons.key, size: 22, color: colors.icon.accent),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  'Rebuilding $who\'s key',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            meHelping
                ? 'Your phone is one of the helpers ($names). It does its part '
                      'whenever Zafe is open or checks in the background.'
                : 'The phones of $names rebuild it on $who\'s new phone when they open '
                      'Zafe. If one of them can\'t for a while, help in their place.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          if (!meHelping && onTakeOver != null)
            for (final (key, name) in helpers) ...[
              const SizedBox(height: AppSpacing.s),
              SizedBox(
                width: double.infinity,
                child: AppButton(
                  expand: true,
                  variant: AppButtonVariant.secondary,
                  size: AppButtonSize.medium,
                  onPressed: () => onTakeOver!(key, name),
                  child: Text('Help instead of $name'),
                ),
              ),
            ],
        ],
      ),
    );
  }
}

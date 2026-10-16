import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/network_config.dart';
import '../../core/formatting/member_label.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/privacy_mask.dart';
import '../../core/theme/app_theme.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/widgets/animated_keyed_column.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../proposals/proposal_status.dart';

/// "Needs you" on Home: what the group is waiting on this member for, as request cards
/// (a payment to approve or send, a seat move to check). Shown only when there is
/// something; cards slide out once handled. Each opens where the action happens.
class NeedsYou extends ConsumerWidget {
  const NeedsYou({super.key, this.height});
  final int? height;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final proposals = ref.watch(proposalsProvider);
    final me = ref.watch(vaultProvider).myKeyHex;
    final names = ref.watch(signerNamesProvider);
    final hide = ref.watch(privacyModeProvider);
    final cards = <Widget>[
      for (final p in proposals.items)
        if (needsMyAction(p, height: height))
          RequestCard(
            key: ValueKey('p:${p.id}'),
            tileKey: p.author,
            who: memberLabel(p.author, me: me, names: names),
            amount: p.payments.isEmpty
                ? null
                : hide
                ? fixedPrivacyMask(length: 3)
                : ZecAmount.fromZatoshi(p.totalZat).activity.amountText,
            detail: _detail(p, height),
            onTap: () => context.push('/proposal/${p.id}'),
            // A vote opens the request (recipient and this phone's check come first);
            // a payment with every approval is sent right away, after the unlock.
            action: p.stage == rust.ProposalStage.approved && !p.needsReapproval
                ? 'Send now'
                : 'Approve or decline',
            onAction:
                p.stage == rust.ProposalStage.approved && !p.needsReapproval
                ? () => _send(context, ref, p.id)
                : () => context.push('/proposal/${p.id}'),
          ),
      for (final m in proposals.seatMoves)
        if (m.oldKeyHex != me && !m.approvals.contains(me))
          RequestCard(
            key: ValueKey('m:${m.oldKeyHex}:${m.newKeyHex}'),
            tileKey: m.oldKeyHex,
            who: memberLabel(m.oldKeyHex, me: me, names: names),
            detail:
                'Moving to a new phone · ${m.approvals.length} of ${m.needed} approved',
            action: 'Compare safety words',
            onTap: () =>
                context.push('/replace-signer', extra: (m.oldKeyHex, m.code)),
          ),
    ];
    return AnimatedSize(
      duration: AnimatedKeyedColumn.duration,
      curve: Curves.easeOutCubic,
      alignment: Alignment.topCenter,
      child: cards.isEmpty
          ? const SizedBox(width: double.infinity)
          : Padding(
              padding: const EdgeInsets.fromLTRB(
                AppSpacing.xs,
                AppSpacing.s,
                AppSpacing.xs,
                AppSpacing.s,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Needs you',
                    style: AppTypography.labelLarge.copyWith(
                      color: colors.text.brand,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.s),
                  AnimatedKeyedColumn(spacing: AppSpacing.s, children: cards),
                ],
              ),
            ),
    );
  }

  /// "Send now" from Home: nothing is left to decide, so after the unlock it sends and
  /// shows the sending screen.
  static Future<void> _send(
    BuildContext context,
    WidgetRef ref,
    String id,
  ) async {
    if (!await confirmUnlock(context, ref, reason: 'Send this payment')) return;
    if (context.mounted) unawaited(context.push('/proposal/$id/send'));
  }

  /// The message, else the recipient; with "expires in ..." when under a day is left.
  static String _detail(rust.ProposalInfo p, int? height) {
    final payment = p.payments.isEmpty ? null : p.payments.first;
    final base = payment == null
        ? 'Back to the vault'
        : p.payments.length > 1
        ? 'To ${p.payments.length} recipients'
        : payment.memo.isNotEmpty
        ? payment.memo
        : 'To ${compactAddress(payment.address)}';
    if (height == null || p.expiryHeight <= 0) return base;
    final secs = blocksToSeconds(height, p.expiryHeight);
    return secs > 0 && secs < 86400
        ? '$base · expires in ${expiresIn(p, height)}'
        : base;
  }
}

/// One request: who it's from, a detail line (the message, or what is asked) and the
/// amount, then a button naming the decision across the card (tonal, so several cards
/// don't drown out the screen's main action). The card itself opens the payment;
/// [onAction] defaults to the same.
class RequestCard extends StatelessWidget {
  const RequestCard({
    super.key,
    required this.tileKey,
    required this.who,
    this.amount,
    required this.detail,
    required this.action,
    required this.onTap,
    this.onAction,
  });

  /// Key hex the signer tile is drawn from.
  final String tileKey;
  final String who;

  /// Without the ticker ("0.8"); the card adds it.
  final String? amount;
  final String detail;
  final String action;
  final VoidCallback onTap;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final strong = AppTypography.labelLarge.copyWith(
      color: colors.text.accent,
      fontWeight: FontWeight.w600,
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.all(AppSpacing.s),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AppTappable(
            onTap: onTap,
            semanticsLabel: '${amount == null ? '' : '$amount. '}$who. $detail',
            child: Row(
              children: [
                SignerTile(keyHex: tileKey, size: 40),
                const SizedBox(width: AppSpacing.s),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // The amount leads (what is asked); who and why under it.
                      if (amount == null)
                        Text(
                          who,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: strong,
                        )
                      else
                        Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(text: amount),
                              TextSpan(
                                text: ' $kZcashDefaultCurrencyTicker',
                                style: TextStyle(
                                  fontSize: 13,
                                  color: colors.text.value,
                                ),
                              ),
                            ],
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: strong.copyWith(
                            fontFamily: 'Space Grotesk',
                            fontSize: 18,
                          ),
                        ),
                      const SizedBox(height: 2),
                      Text(
                        amount == null ? detail : '$who · $detail',
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.bodySmall.copyWith(
                          color: colors.text.secondary,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          AppButton(
            expand: true,
            size: AppButtonSize.medium,
            enabledBackgroundColor: colors.background.brandAlpha,
            enabledLabelColor: colors.text.brand,
            enabledBorderColor: const Color(0x00000000),
            onPressed: onAction ?? onTap,
            child: Text(action),
          ),
        ],
      ),
    );
  }
}

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_loading_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_address_verify_sheet.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import 'proposal_status.dart';

// The parts of the proposal screen below the nav bar, apart from the action buttons.
// Public so tool/screens/ can render them with fake data.

/// Payment card, details (with this device's independent check) and approvals.
class ProposalBody extends ConsumerWidget {
  const ProposalBody({
    super.key,
    required this.proposal,
    required this.members,
    required this.me,
    this.send,
    this.height,
    this.names = const {},
  });
  final rust.ProposalInfo proposal;
  final List<String> members;
  final String? me;
  final SendState? send;

  /// Local names of the signers (key hex → name).
  final Map<String, String> names;

  /// Synced chain tip, to show expiry.
  final int? height;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final p = proposal;
    final progress = send?.progress;
    final sending = send?.running ?? false;
    final expired = proposalExpired(p, height);
    final showReview =
        !expired &&
        (p.stage == rust.ProposalStage.open ||
            p.stage == rust.ProposalStage.approved);
    final txid = p.txid ?? progress?.txid;

    final payment = p.payments.isEmpty ? null : p.payments.first;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (payment == null)
          const SweepCard()
        else
          PaymentCard(
            amountText: ZecAmount.fromZatoshi(p.totalZat).receipt.amountText,
            hidden: ref.watch(privacyModeProvider),
            address: payment.address,
            recipients: p.payments.length,
            strikethrough:
                expired ||
                p.stage == rust.ProposalStage.rejected ||
                p.stage == rust.ProposalStage.cancelled,
            onFullAddress: () => showMobileAddressVerifySheet(
              context,
              title: 'Full address',
              address: payment.address,
            ),
          ),
        if (p.payments.length > 1) ...[
          const SizedBox(height: AppSpacing.md),
          RecipientsCard.fromProposal(p.payments),
        ],
        const SizedBox(height: AppSpacing.md),
        ProposalApprovalsCard(
          proposal: p,
          members: members,
          me: me,
          sending: sending,
          height: height,
          names: names,
        ),
        const SizedBox(height: AppSpacing.md),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          padding: const EdgeInsets.symmetric(
            horizontal: AppSpacing.sm,
            vertical: AppSpacing.md,
          ),
          child: Column(
            children: [
              DetailRow(
                label: 'Proposed by',
                value: memberLabel(
                  p.author,
                  me: me,
                  names: names,
                  withKey: true,
                ),
              ),
              if (p.createdAt > BigInt.zero) ...[
                const DetailDivider(),
                DetailRow(
                  label: 'Created',
                  value: formatTimestamp(p.createdAt),
                ),
              ],
              if (payment != null &&
                  p.payments.length == 1 &&
                  payment.memo.isNotEmpty) ...[
                const DetailDivider(),
                DetailRow(label: 'Message', value: payment.memo),
              ],
              if (showReview) ...[const DetailDivider(), _ReviewRows(id: p.id)],
              if (txid != null) ...[const DetailDivider(), TxIdRow(txid: txid)],
            ],
          ),
        ),
      ],
    );
  }
}

/// Status, the approval threshold as signer dots, and one row per signer with their
/// vote. The signer list is folded away once the payment is finished (sent, rejected
/// or cancelled) and open while votes still matter.
class ProposalApprovalsCard extends StatefulWidget {
  const ProposalApprovalsCard({
    super.key,
    required this.proposal,
    required this.members,
    required this.me,
    this.sending = false,
    this.height,
    this.names = const {},
  });
  final rust.ProposalInfo proposal;
  final List<String> members;
  final String? me;
  final bool sending;
  final int? height;
  final Map<String, String> names;

  @override
  State<ProposalApprovalsCard> createState() => _ProposalApprovalsCardState();
}

class _ProposalApprovalsCardState extends State<ProposalApprovalsCard> {
  bool? _expanded;

  bool get _finished => switch (widget.proposal.stage) {
    _ when proposalExpired(widget.proposal, widget.height) => true,
    rust.ProposalStage.open || rust.ProposalStage.approved => false,
    _ => true,
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final p = widget.proposal;
    final members = widget.members;
    final me = widget.me;
    final sending = widget.sending;
    final expanded = _expanded ?? !_finished;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.fromLTRB(
        AppSpacing.sm,
        AppSpacing.sm,
        AppSpacing.sm,
        AppSpacing.xs,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Wraps (status under the title) rather than squeeze "Approvals" on narrow
          // phones with large system text.
          SizedBox(
            width: double.infinity,
            child: Wrap(
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: AppSpacing.xs,
              runSpacing: AppSpacing.xxs,
              children: [
                Text(
                  'Approvals',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                ProposalStatusChip(
                  proposal: p,
                  sending: sending,
                  height: widget.height,
                ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          SizedBox(
            width: double.infinity,
            child: Wrap(
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: AppSpacing.xs,
              runSpacing: AppSpacing.xxs,
              children: [
                ApprovalDots(
                  approved: p.approvals.length,
                  rejected: p.rejections.length,
                  members: members.length,
                  threshold: p.threshold,
                ),
                Text(
                  // Approvals so far, then the vault's rule (t of n), so a 2-of-3 vault never
                  // reads as "2 of 2".
                  '${p.approvals.length} approved · needs ${p.threshold} of ${members.length}',
                  style: AppTypography.labelMedium.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          const SizedBox(height: AppSpacing.xs),
          Container(height: 1, color: colors.border.regular),
          AppTappable(
            onTap: () => setState(() => _expanded = !expanded),
            semanticsLabel: expanded ? 'Hide signers' : 'Show signers',
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.s),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      expanded ? 'Hide signers' : 'Show signers',
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ),
                  AnimatedRotation(
                    turns: expanded ? -0.25 : 0.25,
                    duration: const Duration(milliseconds: 200),
                    child: AppIcon(
                      AppIcons.chevronForward,
                      size: 16,
                      color: colors.icon.muted,
                    ),
                  ),
                ],
              ),
            ),
          ),
          AnimatedSize(
            duration: const Duration(milliseconds: 200),
            curve: Curves.easeOut,
            alignment: Alignment.topCenter,
            child: expanded
                ? Column(
                    children: [
                      for (final m in members)
                        SignerRow(
                          keyHex: m,
                          me: me,
                          name: widget.names[m],
                          trailing: _VoteTag(
                            approved: p.approvals.contains(m),
                            rejected: p.rejections.contains(m),
                            stage: p.stage,
                          ),
                        ),
                    ],
                  )
                : const SizedBox(width: double.infinity),
          ),
        ],
      ),
    );
  }
}

/// Fee, change and the "verified on this device" line, from `review_proposal`.
class _ReviewRows extends ConsumerWidget {
  const _ReviewRows({required this.id});
  final String id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final review = ref.watch(proposalReviewProvider(id));
    final hide = ref.watch(privacyModeProvider);
    return review.when(
      loading: () => DetailRow(
        label: 'Checking on this device',
        value: '',
        trailing: AppLoadingIcon(size: 16, color: colors.icon.muted),
      ),
      error: (e, _) => AppTappable(
        onTap: () => ref.invalidate(proposalReviewProvider(id)),
        child: DetailRow(
          label: 'Check',
          value: zafeErrorMessage(
            e,
            fallback: 'Couldn\'t check. Tap to retry.',
          ),
          trailing: AppIcon(AppIcons.renew, size: 16, color: colors.icon.muted),
        ),
      ),
      data: (r) => r.verified
          ? Column(
              children: [
                DetailRow(
                  label: 'Network fee',
                  value: amountWithTicker(
                    ZecAmount.fromZatoshi(r.feeZat).fee.amountText,
                    hide: hide,
                  ),
                ),
                const DetailDivider(),
                DetailRow(
                  label: 'Change to vault',
                  value: amountWithTicker(
                    ZecAmount.fromZatoshi(r.changeZat).receipt.amountText,
                    hide: hide,
                  ),
                ),
                const DetailDivider(),
                DetailRow(
                  label: 'Checked on this device',
                  onHelp: () => showDeviceCheckSheet(context),
                  value: 'Matches',
                  trailing: AppIcon(
                    AppIcons.checkCircle,
                    size: 16,
                    color: colors.icon.value,
                    patina: colors.icon.value,
                  ),
                ),
              ],
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                DetailRow(
                  label: 'Checked on this device',
                  onHelp: () => showDeviceCheckSheet(context),
                  value: 'Does not match',
                  trailing: AppIcon(
                    AppIcons.warning,
                    size: 16,
                    color: colors.icon.destructive,
                    patina: colors.icon.destructive,
                  ),
                ),
                const SizedBox(height: AppSpacing.xs),
                Text(
                  'The transaction does not do exactly what the proposal says. Don\'t approve it. (${r.problem})',
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.destructive,
                  ),
                ),
              ],
            ),
    );
  }
}

class _VoteTag extends StatelessWidget {
  const _VoteTag({
    required this.approved,
    required this.rejected,
    required this.stage,
  });
  final bool approved;
  final bool rejected;
  final rust.ProposalStage stage;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    if (approved) {
      return SignerTag(
        icon: AppIcons.checkCircle,
        iconColor: colors.icon.brand,
        label: 'Approved',
        color: colors.text.secondary,
      );
    }
    if (rejected) {
      return SignerTag(
        icon: AppIcons.cross,
        iconColor: colors.icon.destructive,
        label: 'Rejected',
        color: colors.text.secondary,
      );
    }
    // A member who hasn't voted is only "waiting" while their vote can still matter.
    return switch (stage) {
      rust.ProposalStage.open => SignerTag(
        icon: AppIcons.time,
        label: 'Waiting',
        color: colors.text.secondary,
      ),
      rust.ProposalStage.approved => SignerTag(
        label: 'Not needed',
        color: colors.text.muted,
      ),
      _ => SignerTag(label: 'Didn\'t vote', color: colors.text.muted),
    };
  }
}

/// One payment of a batch, as plain data (proposal or draft).
typedef RecipientLine = ({String address, BigInt amountZat, String memo});

/// Every recipient of a batch payment: short address, amount and message, each address
/// tappable in full. [onRemove] (drafts only) shows a remove button per line while there
/// is more than one.
class RecipientsCard extends ConsumerWidget {
  const RecipientsCard({super.key, required this.lines, this.onRemove});

  /// From a draft's `PaymentInput`s or a proposal's `PaymentInfo`s.
  RecipientsCard.fromInputs(
    List<rust.PaymentInput> payments, {
    Key? key,
    void Function(int)? onRemove,
  }) : this(
         key: key,
         lines: [
           for (final p in payments)
             (address: p.address, amountZat: p.amountZat, memo: p.memo),
         ],
         onRemove: onRemove,
       );

  RecipientsCard.fromProposal(List<rust.PaymentInfo> payments, {Key? key})
    : this(
        key: key,
        lines: [
          for (final p in payments)
            (address: p.address, amountZat: p.amountZat, memo: p.memo),
        ],
      );

  final List<RecipientLine> lines;
  final void Function(int index)? onRemove;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final hide = ref.watch(privacyModeProvider);
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.sm,
        vertical: AppSpacing.s,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${lines.length} recipients',
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          for (var i = 0; i < lines.length; i++) ...[
            if (i > 0) const DetailDivider(),
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        AppTappable(
                          onTap: () => showMobileAddressVerifySheet(
                            context,
                            title: 'Recipient ${i + 1}',
                            address: lines[i].address,
                          ),
                          semanticsLabel: 'Recipient ${i + 1} full address',
                          child: Text(
                            compactAddress(lines[i].address),
                            style: AppTypography.labelLarge.copyWith(
                              fontFamily: 'JetBrains Mono',
                              color: colors.text.primary,
                            ),
                          ),
                        ),
                        if (lines[i].memo.isNotEmpty)
                          Text(
                            lines[i].memo,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: AppTypography.bodySmall.copyWith(
                              color: colors.text.secondary,
                            ),
                          ),
                      ],
                    ),
                  ),
                  const SizedBox(width: AppSpacing.xs),
                  Text(
                    amountWithTicker(
                      ZecAmount.fromZatoshi(
                        lines[i].amountZat,
                      ).activity.amountText,
                      hide: hide,
                      maskLength: 3,
                    ),
                    style: AppTypography.labelLarge.copyWith(
                      color: colors.text.accent,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  if (onRemove != null && lines.length > 1) ...[
                    const SizedBox(width: AppSpacing.xs),
                    AppTappable(
                      onTap: () => onRemove!(i),
                      semanticsLabel: 'Remove recipient ${i + 1}',
                      child: AppIcon(
                        AppIcons.cross,
                        size: 18,
                        color: colors.icon.muted,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A proposal with no payments: it spends a cancelled payment's funds back to the vault
/// (`invalidate_proposal`), so that payment can never be sent.
class SweepCard extends StatelessWidget {
  const SweepCard({super.key});

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
              AppIcon(
                AppIcons.shieldKeyhole,
                size: 20,
                color: colors.icon.accent,
              ),
              const SizedBox(width: AppSpacing.xs),
              Text(
                'Move funds back to the vault',
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.accent,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Moves a cancelled payment\'s funds back to this vault so it can never be sent. Only the network fee leaves.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ),
    );
  }
}

/// Explains "Checked on this device": what the phone verified in the transaction itself.
Future<void> showDeviceCheckSheet(
  BuildContext context,
) => showAppMobileSheet<void>(
  context: context,
  builder: (sheet) {
    final colors = sheet.colors;
    Widget point(String title, String body) => Padding(
      padding: const EdgeInsets.only(bottom: AppSpacing.s),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 2),
            child: AppIcon(
              AppIcons.checkCircle,
              size: 16,
              color: colors.icon.value,
              patina: colors.icon.value,
            ),
          ),
          const SizedBox(width: AppSpacing.xs),
          Expanded(
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: '$title ',
                    style: TextStyle(color: colors.text.accent),
                  ),
                  TextSpan(text: body),
                ],
              ),
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
          ),
        ],
      ),
    );
    return MobileModalScaffold(
      title: 'Checked on this device',
      onClose: () => Navigator.of(sheet).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'This phone checked the transaction itself, not just this summary:',
            style: AppTypography.bodyMedium.copyWith(color: colors.text.accent),
          ),
          const SizedBox(height: AppSpacing.sm),
          point('Payments.', 'Recipients, amounts and memos are as shown.'),
          point('Funds.', 'Only the vault\'s money is spent.'),
          point('Change.', 'Everything left over returns to the vault.'),
          point('Fee.', 'The standard network fee.'),
          point('Network.', 'Valid here, with the expected expiry.'),
        ],
      ),
    );
  },
);

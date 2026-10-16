import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/network_config.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/privacy/privacy_mask.dart';
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
import 'proposal_timeline.dart';

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
    final details = <Widget>[
      if (payment != null && p.payments.length == 1 && payment.memo.isNotEmpty)
        DetailRow(label: 'Message', value: payment.memo),
      if (showReview) _ReviewRows(id: p.id),
      if (txid != null) TxIdRow(txid: txid),
    ];
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
        ProposalTimeline(
          steps: timelineSteps(
            p,
            members: members,
            me: me,
            names: names,
            height: height,
            sending: sending,
          ),
        ),
        // Message, this phone's check and the Tx ID, when there are any.
        if (details.isNotEmpty) ...[
          const SizedBox(height: AppSpacing.md),
          MobileSurfaceCard(
            cornerRadius: AppRadii.large,
            padding: const EdgeInsets.symmetric(
              horizontal: AppSpacing.sm,
              vertical: AppSpacing.md,
            ),
            child: Column(
              children: [
                ...details.expand((w) => [const DetailDivider(), w]).skip(1),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

/// A payment that waits for this member's vote, shown as a request: who asks, how
/// much, to whom, and whether this phone's own check of the transaction agrees.
/// Everything else (fee, change, signers) is folded under "Details". The actions (Approve,
/// Decline) are pinned below it by the screen.
class PaymentRequestView extends ConsumerStatefulWidget {
  const PaymentRequestView({
    super.key,
    required this.proposal,
    required this.members,
    required this.me,
    required this.names,
    this.height,
  });
  final rust.ProposalInfo proposal;
  final List<String> members;
  final String? me;
  final Map<String, String> names;
  final int? height;

  @override
  ConsumerState<PaymentRequestView> createState() => _PaymentRequestViewState();
}

class _PaymentRequestViewState extends ConsumerState<PaymentRequestView> {
  bool _details = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final p = widget.proposal;
    final hide = ref.watch(privacyModeProvider);
    final payment = p.payments.isEmpty ? null : p.payments.first;
    final who = memberLabel(p.author, me: widget.me, names: widget.names);
    final soon = widget.height == null || p.expiryHeight <= 0
        ? null
        : blocksToSeconds(widget.height!, p.expiryHeight) < 86400
        ? expiresIn(p, widget.height!)
        : null;
    final display = TextStyle(
      fontFamily: 'Space Grotesk',
      fontWeight: FontWeight.w500,
      color: colors.text.accent,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SizedBox(height: AppSpacing.md),
        Center(child: SignerTile(keyHex: p.author, size: 56)),
        const SizedBox(height: AppSpacing.s),
        Text(
          payment == null ? '$who asks to move funds' : '$who asks to send',
          textAlign: TextAlign.center,
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        FittedBox(
          fit: BoxFit.scaleDown,
          child: Text.rich(
            TextSpan(
              children: [
                TextSpan(
                  text: hide
                      ? fixedPrivacyMask()
                      : ZecAmount.fromZatoshi(p.totalZat).receipt.amountText,
                  style: display.copyWith(
                    fontSize: 48,
                    height: 1.05,
                    letterSpacing: -1.8,
                  ),
                ),
                TextSpan(
                  text: ' $kZcashDefaultCurrencyTicker',
                  style: display.copyWith(
                    fontSize: 22,
                    color: colors.text.value,
                  ),
                ),
              ],
            ),
            maxLines: 1,
          ),
        ),
        const SizedBox(height: AppSpacing.s),
        if (payment == null)
          Text(
            'Back to the vault',
            textAlign: TextAlign.center,
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          )
        else
          Center(
            child: AppTappable(
              semanticsLabel: 'Show the full address',
              onTap: () => showMobileAddressVerifySheet(
                context,
                title: 'Full address',
                address: payment.address,
              ),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: AppSpacing.s,
                  vertical: AppSpacing.xs,
                ),
                decoration: BoxDecoration(
                  color: colors.background.neutralSubtleOpacity,
                  borderRadius: BorderRadius.circular(AppRadii.full),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    AppIcon(
                      AppIcons.shieldKeyhole,
                      size: 14,
                      color: colors.icon.brand,
                    ),
                    const SizedBox(width: AppSpacing.xxs),
                    Flexible(
                      child: Text(
                        p.payments.length > 1
                            ? 'To ${p.payments.length} recipients'
                            : 'To ${compactAddress(payment.address)}',
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.codeSmall.copyWith(
                          color: colors.text.accent,
                        ),
                      ),
                    ),
                    const SizedBox(width: AppSpacing.xxs),
                    AppIcon(
                      AppIcons.chevronForward,
                      size: 14,
                      color: colors.icon.muted,
                    ),
                  ],
                ),
              ),
            ),
          ),
        if (payment != null &&
            p.payments.length == 1 &&
            payment.memo.isNotEmpty) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            '"${payment.memo}"',
            textAlign: TextAlign.center,
            style: AppTypography.bodyMedium.copyWith(color: colors.text.accent),
          ),
        ],
        if (p.payments.length > 1) ...[
          const SizedBox(height: AppSpacing.md),
          RecipientsCard.fromProposal(p.payments),
        ],
        const SizedBox(height: AppSpacing.lg),
        _DeviceCheck(id: p.id),
        if (soon != null) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Expires in $soon',
            textAlign: TextAlign.center,
            style: AppTypography.bodySmall.copyWith(color: colors.text.warning),
          ),
        ],
        const SizedBox(height: AppSpacing.lg),
        AppTappable(
          semanticsLabel: _details ? 'Hide details' : 'Show details',
          onTap: () => setState(() => _details = !_details),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  'Details',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
                const SizedBox(width: AppSpacing.xxs),
                // The forward chevron turned down (closed) or up (open).
                AnimatedRotation(
                  turns: _details ? 0.75 : 0.25,
                  duration: const Duration(milliseconds: 200),
                  curve: Curves.easeOutCubic,
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
          duration: const Duration(milliseconds: 220),
          curve: Curves.easeOutCubic,
          alignment: Alignment.topCenter,
          child: !_details
              ? const SizedBox(width: double.infinity)
              : Padding(
                  padding: const EdgeInsets.only(top: AppSpacing.s),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      ProposalTimeline(
                        steps: timelineSteps(
                          p,
                          members: widget.members,
                          me: widget.me,
                          names: widget.names,
                          height: widget.height,
                        ),
                      ),
                      const SizedBox(height: AppSpacing.md),
                      MobileSurfaceCard(
                        cornerRadius: AppRadii.large,
                        padding: const EdgeInsets.symmetric(
                          horizontal: AppSpacing.sm,
                          vertical: AppSpacing.md,
                        ),
                        child: Column(
                          children: [_ReviewRows(id: p.id, withCheck: false)],
                        ),
                      ),
                    ],
                  ),
                ),
        ),
      ],
    );
  }
}

/// The one trust line of a request: this phone's own check of the transaction. Tap for
/// what was checked. A mismatch is spelled out (never approve it).
class _DeviceCheck extends ConsumerWidget {
  const _DeviceCheck({required this.id});
  final String id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final review = ref.watch(proposalReviewProvider(id));
    Widget line(Widget icon, String text, Color color) => Center(
      child: AppTappable(
        semanticsLabel: text,
        onTap: () => showDeviceCheckSheet(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: AppSpacing.xxs),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              icon,
              const SizedBox(width: AppSpacing.xs),
              Flexible(
                child: Text(
                  text,
                  style: AppTypography.labelLarge.copyWith(color: color),
                ),
              ),
            ],
          ),
        ),
      ),
    );
    return review.when(
      loading: () => line(
        AppLoadingIcon(size: 16, color: colors.icon.muted),
        'Checking on this phone...',
        colors.text.secondary,
      ),
      error: (e, _) => AppTappable(
        onTap: () => ref.invalidate(proposalReviewProvider(id)),
        child: Text(
          zafeErrorMessage(e, fallback: 'Couldn\'t check. Tap to retry.'),
          textAlign: TextAlign.center,
          style: AppTypography.bodySmall.copyWith(color: colors.text.warning),
        ),
      ),
      data: (r) => r.verified
          ? line(
              AppIcon(
                AppIcons.checkCircle,
                size: 18,
                color: colors.icon.value,
                patina: colors.icon.value,
              ),
              'Checked on this phone',
              colors.text.accent,
            )
          : MobileSurfaceCard(
              cornerRadius: AppRadii.large,
              child: Text(
                'This transaction does not do what the request says. Don\'t approve it. (${r.problem})',
                style: AppTypography.bodySmall.copyWith(
                  color: colors.text.destructive,
                ),
              ),
            ),
    );
  }
}

/// Fee, change and the "verified on this device" line, from `review_proposal`.
class _ReviewRows extends ConsumerWidget {
  const _ReviewRows({required this.id, this.withCheck = true});
  final String id;

  /// The "Checked on this device" row too (a request shows the check above instead).
  final bool withCheck;

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
                if (withCheck) ...[
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

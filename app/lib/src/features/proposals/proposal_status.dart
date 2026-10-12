import 'package:flutter/widgets.dart';

import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../rust/api/proposals.dart' as rust;

/// Seconds per block (Zcash target spacing since Blossom).
const _blockSeconds = 75;

/// Seconds per block from the NU7 network upgrade on.
const _blockSecondsNu7 = 25;

/// Seconds the chain takes to go from block `from` to block `to`: 75 s blocks before the
/// NU7 upgrade at `nu7Height` (null: not known, so every block counts 75 s) and 25 s after.
int blocksToSeconds(int from, int to, {int? nu7Height}) {
  if (to <= from) return 0;
  if (nu7Height == null) return (to - from) * _blockSeconds;
  final before = (nu7Height.clamp(from, to) - from) * _blockSeconds;
  final after = (to - nu7Height.clamp(from, to)) * _blockSecondsNu7;
  return before + after;
}

/// Whether an unsent proposal can no longer be mined: the chain (`height`, the synced
/// tip) has reached its transaction's expiry height. Unknown height: not expired.
bool proposalExpired(rust.ProposalInfo p, int? height) =>
    (p.stage == rust.ProposalStage.open ||
        p.stage == rust.ProposalStage.approved) &&
    p.expiryHeight > 0 &&
    height != null &&
    height >= p.expiryHeight;

/// "about 6 days" until an unsent proposal expires, from the synced tip `height`.
String expiresIn(rust.ProposalInfo p, int height, {int? nu7Height}) {
  final seconds = blocksToSeconds(height, p.expiryHeight, nu7Height: nu7Height);
  final minutes = (seconds / 60).round();
  final hours = (seconds / 3600).round();
  final days = (seconds / 86400).round();
  String n(int v, String unit) => '$v $unit${v == 1 ? '' : 's'}';
  if (minutes < 60) return 'about ${n(minutes < 1 ? 1 : minutes, 'minute')}';
  if (hours < 36) return 'about ${n(hours, 'hour')}';
  return 'about ${n(days, 'day')}';
}

/// Title for a proposal from this member's point of view (sentence case).
String proposalTitle(rust.ProposalInfo p, {int? height}) =>
    proposalExpired(p, height) ? 'Expired' : _title(p);

String _title(rust.ProposalInfo p) => switch (p.stage) {
  rust.ProposalStage.open =>
    p.myVote == rust.MyVote.none
        ? 'Needs your approval'
        : 'Waiting for approvals',
  rust.ProposalStage.approved => 'Ready to send',
  rust.ProposalStage.rejected => 'Rejected',
  rust.ProposalStage.cancelled => 'Cancelled',
  rust.ProposalStage.sent => 'Sent',
};

String _icon(rust.ProposalInfo p, int? height) =>
    proposalExpired(p, height) ? AppIcons.time : _stageIcon(p);

String _stageIcon(rust.ProposalInfo p) => switch (p.stage) {
  rust.ProposalStage.open =>
    p.myVote == rust.MyVote.none ? AppIcons.editFilled : AppIcons.time,
  rust.ProposalStage.approved => AppIcons.plane,
  rust.ProposalStage.rejected || rust.ProposalStage.cancelled => AppIcons.cross,
  rust.ProposalStage.sent => AppIcons.checkCircle,
};

const _months = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

/// "May 29, 13:40" (activity timestamp).
String formatTimestamp(BigInt unixSeconds) {
  final t = DateTime.fromMillisecondsSinceEpoch(unixSeconds.toInt() * 1000);
  String two(int n) => n.toString().padLeft(2, '0');
  return '${_months[t.month - 1]} ${t.day}, ${two(t.hour)}:${two(t.minute)}';
}

class ProposalStatusChip extends StatelessWidget {
  const ProposalStatusChip({
    super.key,
    required this.proposal,
    this.sending = false,
    this.height,
  });
  final rust.ProposalInfo proposal;
  final bool sending;

  /// Synced chain tip, to show expiry (unknown: never shown as expired).
  final int? height;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final (icon, label, color) = sending
        ? (AppIcons.loader, 'Sending', colors.text.secondary)
        : proposalExpired(proposal, height)
        ? (AppIcons.time, 'Expired', colors.text.muted)
        : switch (proposal.stage) {
            rust.ProposalStage.open => (
              AppIcons.time,
              // The count is on the line below the chip.
              'Collecting votes',
              colors.text.secondary,
            ),
            rust.ProposalStage.approved => (
              AppIcons.check,
              'Approved',
              colors.text.value,
            ),
            rust.ProposalStage.rejected => (
              AppIcons.cross,
              'Rejected',
              colors.text.destructive,
            ),
            rust.ProposalStage.cancelled => (
              AppIcons.cross,
              'Cancelled',
              colors.text.muted,
            ),
            rust.ProposalStage.sent => (
              AppIcons.checkCircle,
              'Completed',
              colors.text.value,
            ),
          };
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        AppIcon(icon, size: 13, color: color, patina: color),
        const SizedBox(width: AppSpacing.xxs),
        Text(label, style: AppTypography.labelMedium.copyWith(color: color)),
      ],
    );
  }
}

/// 44px activity row for a proposal.
class ProposalRow extends StatelessWidget {
  const ProposalRow({
    super.key,
    required this.proposal,
    required this.onTap,
    this.hideAmount = false,
    this.height,
  });
  final rust.ProposalInfo proposal;
  final bool hideAmount;
  final VoidCallback onTap;

  /// Synced chain tip, to show expiry.
  final int? height;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final p = proposal;
    final expired = proposalExpired(p, height);
    final needsMe =
        !expired &&
        p.stage == rust.ProposalStage.open &&
        p.myVote == rust.MyVote.none;
    final subtitle = expired
        ? (p.createdAt > BigInt.zero ? formatTimestamp(p.createdAt) : '')
        : switch (p.stage) {
            rust.ProposalStage.open =>
              '${p.approvals.length} of ${p.threshold} approved',
            rust.ProposalStage.approved => 'Approved',
            _ => p.createdAt > BigInt.zero ? formatTimestamp(p.createdAt) : '',
          };
    return AppTappable(
      onTap: onTap,
      child: SizedBox(
        height: 44,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: AppSpacing.xxs),
          child: Row(
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: needsMe
                      ? colors.background.brandAlpha
                      : colors.background.neutralSubtleOpacity,
                  shape: BoxShape.circle,
                ),
                alignment: Alignment.center,
                // A payment that needs your vote: the solid accent icon.
                child: AppIcon(
                  _icon(p, height),
                  size: 18,
                  color: needsMe ? colors.icon.brand : colors.icon.regular,
                  active: needsMe,
                ),
              ),
              const SizedBox(width: AppSpacing.xs),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      proposalTitle(p, height: height),
                      style: AppTypography.labelLarge.copyWith(
                        color: colors.text.accent,
                      ),
                    ),
                    const SizedBox(height: AppSpacing.xxs),
                    Text(
                      subtitle,
                      style: AppTypography.labelLarge.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ],
                ),
              ),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 128),
                child: Text(
                  p.payments.isEmpty
                      ? 'To vault'
                      : amountWithTicker(
                          ZecAmount.fromZatoshi(p.totalZat).activity.amountText,
                          hide: hideAmount,
                          maskLength: 3, // short mask for list rows
                        ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppTypography.labelLarge.copyWith(
                    color:
                        expired ||
                            p.stage == rust.ProposalStage.rejected ||
                            p.stage == rust.ProposalStage.cancelled
                        ? colors.text.muted
                        : colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

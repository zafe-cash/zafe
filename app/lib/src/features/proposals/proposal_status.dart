import 'package:flutter/widgets.dart';

import '../../core/widgets/row_amount.dart';
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

/// A payment waiting for this member: a vote, or (once every signature is in and nobody
/// is auto-sending) a send. Home's "Needs you" and the notifications use the same rule.
bool needsMyAction(rust.ProposalInfo p, {int? height}) =>
    !proposalExpired(p, height) &&
    (p.needsReapproval ||
        (p.stage == rust.ProposalStage.open && p.myVote == rust.MyVote.none) ||
        (p.stage == rust.ProposalStage.approved && !(p.ready && p.autoSend)));

/// " · expires in about 5 hours" when an unsent payment has less than a day left, else ''.
String _soon(rust.ProposalInfo p, int? height) {
  if (height == null || p.expiryHeight <= 0) return '';
  final secs = blocksToSeconds(height, p.expiryHeight);
  return secs > 0 && secs < 86400
      ? ' · expires in ${expiresIn(p, height)}'
      : '';
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
    final faded =
        expired ||
        p.stage == rust.ProposalStage.rejected ||
        p.stage == rust.ProposalStage.cancelled;
    final needsMe = needsMyAction(p, height: height);
    final subtitle = expired
        ? (p.createdAt > BigInt.zero ? formatTimestamp(p.createdAt) : '')
        : switch (p.stage) {
            rust.ProposalStage.open =>
              '${p.approvals.length} of ${p.threshold} approved'
                  '${_soon(p, height)}',
            rust.ProposalStage.approved => 'Approved${_soon(p, height)}',
            _ => p.createdAt > BigInt.zero ? formatTimestamp(p.createdAt) : '',
          };
    return AppTappable(
      onTap: onTap,
      // A minimum: with large system text the two lines need more than 44.
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 44),
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
                child: p.payments.isEmpty
                    ? Text(
                        'To vault',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.labelLarge.copyWith(
                          color: colors.text.secondary,
                        ),
                      )
                    : RowAmount(
                        amountWithTicker(
                          ZecAmount.fromZatoshi(p.totalZat).activity.amountText,
                          hide: hideAmount,
                          maskLength: 3, // short mask for list rows
                        ),
                        color: faded ? colors.text.muted : colors.text.accent,
                        tickerColor: faded
                            ? colors.text.muted
                            : colors.text.value,
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

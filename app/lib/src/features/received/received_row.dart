import 'package:flutter/widgets.dart';

import '../../core/widgets/row_amount.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../rust/api/received.dart' as rust;
import '../proposals/proposal_status.dart' show formatTimestamp;

const kCoinbaseMaturity = 100;

String receivedStatus(rust.ReceivedInfo r) {
  if (r.minedHeight == 0) return 'Pending';
  if (r.isCoinbase && r.confirmations < kCoinbaseMaturity) {
    return 'Maturing (${r.confirmations} of $kCoinbaseMaturity)';
  }
  return 'Confirmed';
}

String receivedAmountText(
  rust.ReceivedInfo r, {
  required bool hide,
  bool detail = false,
}) {
  final amount = ZecAmount.fromZatoshi(r.amountZat);
  return amountWithTicker(
    '+${(detail ? amount.receipt : amount.activity).amountText}',
    hide: hide,
    maskLength: detail ? 6 : 3,
  );
}

class ReceivedRow extends StatelessWidget {
  const ReceivedRow({
    super.key,
    required this.received,
    required this.onTap,
    this.hideAmount = false,
  });
  final rust.ReceivedInfo received;
  final bool hideAmount;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final r = received;
    final subtitle = r.minedHeight == 0
        ? 'Pending'
        : r.blockTimeSecs > 0
        ? formatTimestamp(BigInt.from(r.blockTimeSecs))
        : 'Confirmed';
    return AppTappable(
      onTap: onTap,
      semanticsLabel: 'Received payment',
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
                  color: colors.background.valueAlpha,
                  shape: BoxShape.circle,
                ),
                alignment: Alignment.center,
                child: AppIcon(
                  AppIcons.arrowDown,
                  size: 18,
                  color: colors.text.value,
                  patina: colors.text.value,
                ),
              ),
              const SizedBox(width: AppSpacing.xs),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Received',
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
                child: RowAmount(
                  receivedAmountText(r, hide: hideAmount),
                  color: r.minedHeight == 0
                      ? colors.text.secondary
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

import 'package:flutter/widgets.dart';

import '../config/network_config.dart';
import '../theme/app_theme.dart';

class RowAmount extends StatelessWidget {
  const RowAmount(
    this.text, {
    super.key,
    required this.color,
    this.tickerColor,
  });

  final String text;
  final Color color;

  final Color? tickerColor;

  @override
  Widget build(BuildContext context) {
    const suffix = ' $kZcashDefaultCurrencyTicker';
    final hasTicker = text.endsWith(suffix);
    return Text.rich(
      TextSpan(
        children: [
          TextSpan(
            text: hasTicker
                ? text.substring(0, text.length - suffix.length)
                : text,
          ),
          if (hasTicker)
            TextSpan(
              text: suffix,
              style: AppTypography.amountTicker.copyWith(
                color: tickerColor ?? color,
              ),
            ),
        ],
      ),
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      textAlign: TextAlign.end,
      style: AppTypography.amount.copyWith(color: color),
    );
  }
}

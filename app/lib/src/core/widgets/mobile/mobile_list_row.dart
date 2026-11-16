import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';
import '../app_icon.dart';

class MobileListRow extends StatelessWidget {
  const MobileListRow({
    required this.label,
    this.leading,
    this.labelBadge,
    this.value,
    this.trailing,
    this.showChevron = false,
    this.onTap,
    this.enabled = true,
    this.labelColor,
    this.valueColor,
    this.chevronColor,
    this.minRowHeight = minHeight,
    this.textStyle,
    this.valueTextStyle,
    super.key,
  });

  static const minHeight = 52.0;

  final Widget? leading;
  final String label;

  final Widget? labelBadge;

  final Color? labelColor;

  final Color? valueColor;

  final Color? chevronColor;

  final double minRowHeight;

  final TextStyle? textStyle;

  final String? value;

  final TextStyle? valueTextStyle;

  final Widget? trailing;
  final bool showChevron;
  final VoidCallback? onTap;

  final bool enabled;

  static bool _stacked(BuildContext context) =>
      MediaQuery.textScalerOf(context).scale(10) >= 15;

  @override
  Widget build(BuildContext context) {
    if (value == null || _stacked(context)) {
      return _row(context, stacked: value != null);
    }
    return LayoutBuilder(
      builder: (context, box) =>
          _row(context, stacked: !_fitsOneLine(context, box.maxWidth)),
    );
  }

  bool _fitsOneLine(BuildContext context, double width) {
    double measure(String text, TextStyle style) {
      final painter = TextPainter(
        text: TextSpan(text: text, style: style),
        textScaler: MediaQuery.textScalerOf(context),
        textDirection: Directionality.of(context),
        maxLines: 1,
      )..layout();
      final w = painter.width;
      painter.dispose();
      return w;
    }

    final labelWidth = measure(label, textStyle ?? AppTypography.bodyMedium);
    final valueWidth = measure(
      value!,
      valueTextStyle ?? AppTypography.bodyMedium,
    );
    final extras =
        (leading == null ? 0 : 36) +
        (labelBadge == null ? 0 : 28) +
        (trailing != null || showChevron ? 32 : 0);
    return labelWidth + AppSpacing.xs + math.min(valueWidth, 40) + extras <=
        width;
  }

  Widget _row(BuildContext context, {required bool stacked}) {
    final colors = context.colors;
    final labelColor =
        this.labelColor ??
        (enabled ? colors.text.accent : colors.text.disabled);
    final valueColor =
        this.valueColor ??
        (enabled ? colors.text.secondary : colors.text.disabled);
    final labelTextStyle = textStyle ?? AppTypography.bodyMedium;
    final valueTextStyle = this.valueTextStyle ?? AppTypography.bodyMedium;

    final row = ConstrainedBox(
      constraints: BoxConstraints(minHeight: minRowHeight),
      child: Row(
        children: [
          if (leading != null) ...[
            leading!,
            const SizedBox(width: AppSpacing.s),
          ],
          if (value != null && stacked)
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          label,
                          style: labelTextStyle.copyWith(color: labelColor),
                        ),
                      ),
                      if (labelBadge != null) ...[
                        const SizedBox(width: AppSpacing.xxs),
                        labelBadge!,
                      ],
                    ],
                  ),
                  Text(
                    value!,
                    overflow: TextOverflow.ellipsis,
                    style: valueTextStyle.copyWith(color: valueColor),
                  ),
                ],
              ),
            )
          else if (value != null) ...[
            Text(label, style: labelTextStyle.copyWith(color: labelColor)),
            if (labelBadge != null) ...[
              const SizedBox(width: AppSpacing.xxs),
              labelBadge!,
            ],
            const SizedBox(width: AppSpacing.xs),
            Expanded(
              child: Text(
                value!,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.right,
                style: valueTextStyle.copyWith(color: valueColor),
              ),
            ),
          ] else if (labelBadge != null)
            Expanded(
              child: Row(
                children: [
                  Flexible(
                    child: Text(
                      label,
                      overflow: TextOverflow.ellipsis,
                      style: labelTextStyle.copyWith(color: labelColor),
                    ),
                  ),
                  const SizedBox(width: AppSpacing.xxs),
                  labelBadge!,
                ],
              ),
            )
          else
            Expanded(
              child: Text(
                label,
                overflow: TextOverflow.ellipsis,
                style: labelTextStyle.copyWith(color: labelColor),
              ),
            ),
          if (trailing != null) ...[
            const SizedBox(width: AppSpacing.xs),
            trailing!,
          ] else if (showChevron) ...[
            const SizedBox(width: AppSpacing.xs),
            AppIcon(
              AppIcons.chevronForward,
              size: AppIconSize.medium,
              color: enabled
                  ? chevronColor ?? colors.icon.muted
                  : colors.icon.disabled,
            ),
          ],
        ],
      ),
    );

    if (onTap == null || !enabled) return row;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: row,
    );
  }
}

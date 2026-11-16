import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';
import '../app_icon.dart';

class MobileReviewInfoRow extends StatelessWidget {
  const MobileReviewInfoRow({
    required this.label,
    required this.value,
    required this.leading,
    this.bottom,
    this.strikethrough = false,
    super.key,
  });

  final String label;
  final String value;
  final Widget leading;
  final Widget? bottom;
  final bool strikethrough;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 90),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          SizedBox(width: 40, child: Center(child: leading)),
          const SizedBox(width: AppSpacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SizedBox(
                  height: 24,
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: Text(
                      label,
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  value,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppTypography.headlineLarge.copyWith(
                    color: colors.text.accent,
                    decoration: strikethrough
                        ? TextDecoration.lineThrough
                        : TextDecoration.none,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                SizedBox(height: 24, child: bottom ?? const SizedBox()),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class MobileReviewFlowArrow extends StatelessWidget {
  const MobileReviewFlowArrow({super.key});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 40,
      child: Center(
        child: AppIcon(
          AppIcons.arrowDown,
          size: AppIconSize.large,
          color: context.colors.icon.accent,
        ),
      ),
    );
  }
}

class MobileReviewZecBadge extends StatelessWidget {
  const MobileReviewZecBadge({super.key});

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(AppRadii.full),
      child: Image.asset(
        'assets/swap/tokens/zec.png',
        width: 32,
        height: 32,
        fit: BoxFit.cover,
      ),
    );
  }
}

class MobileReviewIconBadge extends StatelessWidget {
  const MobileReviewIconBadge({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 40,
      height: 40,
      decoration: BoxDecoration(
        color: context.colors.background.neutralSubtleOpacity,
        shape: BoxShape.circle,
      ),
      child: Center(child: child),
    );
  }
}

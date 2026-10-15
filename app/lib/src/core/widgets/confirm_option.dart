import 'package:flutter/material.dart';

import '../theme/app_theme.dart';
import 'app_icon.dart';
import 'app_tappable.dart';

/// A tappable row the user ticks to confirm they checked something (safety words).
class ConfirmOption extends StatelessWidget {
  const ConfirmOption({
    super.key,
    required this.selected,
    required this.label,
    required this.onTap,
  });
  final bool selected;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return AppTappable(
      onTap: onTap,
      child: Container(
        height: 64,
        padding: const EdgeInsets.symmetric(horizontal: AppSpacing.sm),
        decoration: BoxDecoration(
          color: colors.background.ground,
          borderRadius: BorderRadius.circular(AppRadii.medium),
          border: Border.all(
            color: selected ? colors.border.strong : colors.border.subtle,
            width: selected ? 1.5 : 1,
          ),
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                label,
                style: AppTypography.bodyMediumStrong.copyWith(
                  color: colors.text.accent,
                ),
              ),
            ),
            Container(
              width: 24,
              height: 24,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: selected
                    ? colors.background.inverse
                    : colors.background.neutralSubtleOpacity,
              ),
              alignment: Alignment.center,
              child: selected
                  ? AppIcon(
                      AppIcons.check,
                      size: 14,
                      color: colors.icon.inverse,
                    )
                  : null,
            ),
          ],
        ),
      ),
    );
  }
}

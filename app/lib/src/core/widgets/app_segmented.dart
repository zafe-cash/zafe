import 'package:flutter/services.dart';
import 'package:flutter/material.dart' show Tooltip;
import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';
import 'app_icon.dart';

/// A few mutually exclusive choices shown as icons in one pill; the highlight slides to
/// the chosen one (180 ms). Each choice keeps a label for screen readers and a tooltip
/// on long press.
class AppSegmented<T> extends StatelessWidget {
  const AppSegmented({
    super.key,
    required this.value,
    required this.options,
    required this.onChanged,
  });

  final T value;

  /// (value, icon, label).
  final List<(T, String, String)> options;
  final ValueChanged<T> onChanged;

  static const _cell = 40.0;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final index = options
        .indexWhere((o) => o.$1 == value)
        .clamp(0, options.length);
    return Container(
      height: _cell,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: colors.background.neutralSubtleOpacity,
        borderRadius: BorderRadius.circular(_cell / 2),
      ),
      child: SizedBox(
        width: (_cell - 2) * options.length,
        child: Stack(
          children: [
            AnimatedAlign(
              duration: const Duration(milliseconds: 180),
              curve: Curves.easeOutCubic,
              alignment: Alignment(
                options.length == 1 ? 0 : -1 + 2 * index / (options.length - 1),
                0,
              ),
              child: Container(
                width: _cell - 6,
                height: _cell - 6,
                decoration: BoxDecoration(
                  color: colors.background.raised,
                  shape: BoxShape.circle,
                ),
              ),
            ),
            Row(
              children: [
                for (final (v, icon, label) in options)
                  Expanded(
                    child: Semantics(
                      label: label,
                      selected: v == value,
                      button: true,
                      child: Tooltip(
                        message: label,
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: () {
                            if (v == value) return;
                            HapticFeedback.selectionClick();
                            onChanged(v);
                          },
                          child: Center(
                            child: AppIcon(
                              icon,
                              size: 18,
                              color: v == value
                                  ? colors.icon.accent
                                  : colors.icon.muted,
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

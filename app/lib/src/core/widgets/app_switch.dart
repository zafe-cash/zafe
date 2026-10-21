import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';
import 'app_loading_icon.dart';

/// An on/off switch: the thumb slides and the track eases to the brand colour (160 ms),
/// with a selection tick. [busy] puts a spinner in the thumb while the change takes
/// effect (Tor connecting). `onChanged == null` disables it.
class AppSwitch extends StatelessWidget {
  const AppSwitch({
    super.key,
    required this.value,
    required this.onChanged,
    this.busy = false,
    this.semanticsLabel,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final bool busy;
  final String? semanticsLabel;

  static const _motion = Duration(milliseconds: 160);

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final enabled = onChanged != null;
    return Semantics(
      toggled: value,
      enabled: enabled,
      label: semanticsLabel,
      onTap: enabled ? () => onChanged!(!value) : null,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: enabled
            ? () {
                HapticFeedback.selectionClick();
                onChanged!(!value);
              }
            : null,
        child: Padding(
          // A 48 px tall tap target around the 28 px track.
          padding: const EdgeInsets.symmetric(vertical: 10),
          child: AnimatedOpacity(
            duration: _motion,
            opacity: enabled ? 1 : 0.5,
            child: AnimatedContainer(
              duration: _motion,
              curve: Curves.easeOutCubic,
              width: 48,
              height: 28,
              padding: const EdgeInsets.all(3),
              decoration: BoxDecoration(
                color: value
                    ? colors.button.primary.bg
                    : colors.background.neutralSubtleOpacity,
                borderRadius: BorderRadius.circular(14),
                border: Border.all(
                  color: value
                      ? const Color(0x00000000)
                      : colors.border.regular,
                ),
              ),
              child: AnimatedAlign(
                duration: _motion,
                curve: Curves.easeOutCubic,
                alignment: value ? Alignment.centerRight : Alignment.centerLeft,
                child: Container(
                  width: 22,
                  height: 22,
                  decoration: BoxDecoration(
                    color: value
                        ? colors.button.primary.label
                        : colors.icon.muted,
                    shape: BoxShape.circle,
                  ),
                  alignment: Alignment.center,
                  child: busy
                      ? AppLoadingIcon(
                          size: 14,
                          color: colors.button.primary.bg,
                        )
                      : null,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

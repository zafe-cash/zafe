import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../feedback/app_haptics.dart';
import '../platform/sensitive_clipboard.dart';
import '../theme/app_theme.dart';
import 'app_button.dart';
import 'app_icon.dart';

/// Copies [text]; the button itself confirms (its icon turns into a check and the label
/// reads "Copied" for a moment), with a light tap. [sensitive] for secrets: see
/// [copySecret].
class CopyButton extends StatefulWidget {
  const CopyButton({
    super.key,
    required this.label,
    required this.text,
    this.sensitive = false,
    this.variant = AppButtonVariant.ghost,
    this.size = AppButtonSize.large,
    this.expand = true,
  });

  final String label;
  final String text;
  final bool sensitive;
  final AppButtonVariant variant;
  final AppButtonSize size;
  final bool expand;

  static const shown = Duration(milliseconds: 1400);

  @override
  State<CopyButton> createState() => _CopyButtonState();
}

class _CopyButtonState extends State<CopyButton> {
  bool _copied = false;
  Timer? _reset;

  @override
  void dispose() {
    _reset?.cancel();
    super.dispose();
  }

  Future<void> _copy() async {
    try {
      if (widget.sensitive) {
        await copySecret(widget.text);
      } else {
        await Clipboard.setData(ClipboardData(text: widget.text));
      }
    } catch (_) {
      return;
    }
    unawaited(AppHaptics.copy());
    if (!mounted) return;
    setState(() => _copied = true);
    _reset?.cancel();
    _reset = Timer(CopyButton.shown, () {
      if (mounted) setState(() => _copied = false);
    });
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return AppButton(
      expand: widget.expand,
      variant: widget.variant,
      size: widget.size,
      onPressed: _copy,
      leading: AnimatedSwitcher(
        duration: const Duration(milliseconds: 160),
        transitionBuilder: (child, a) =>
            ScaleTransition(scale: a, child: child),
        child: _copied
            ? AppIcon(
                AppIcons.check,
                key: const ValueKey('done'),
                size: 20,
                color: colors.icon.value,
              )
            : const AppIcon(AppIcons.copy, key: ValueKey('copy'), size: 20),
      ),
      child: Text(_copied ? 'Copied' : widget.label),
    );
  }
}

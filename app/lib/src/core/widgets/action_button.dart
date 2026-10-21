import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../errors/zafe_error_copy.dart';
import '../theme/app_theme.dart';
import 'app_button.dart';
import 'app_icon.dart';
import 'app_loading_icon.dart';

/// A button for an action that takes a moment and can fail (approve, propose, save):
/// it shows what is happening instead of a toast. Idle → working (a spinner inside the
/// button, which keeps its size) → done (a check with [doneLabel], briefly, then
/// [onDone]) or failed (a short shake and the error under the button, until the next
/// try). [onPressed] throws to fail; its error is shown with [zafeErrorMessage]. It
/// throws [ActionCancelled] when the member backed out (e.g. dismissed the unlock):
/// back to idle, no message.
class ActionButton extends StatefulWidget {
  const ActionButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
    this.doneLabel,
    this.busyLabel,
    this.resetAfterDone = false,
    this.onDone,
    this.errorFallback,
    this.variant = AppButtonVariant.primary,
    this.size = AppButtonSize.large,
    this.enabled = true,
  });

  final String label;
  final String? icon;

  /// Shown with a check once [onPressed] completes; without it the button goes back to
  /// idle at once.
  final String? doneLabel;

  /// Shown while working, for steps slow enough to need words ("Decrypting...").
  final String? busyLabel;

  /// Back to idle after "done" (the screen stays, e.g. "Copied"); otherwise it keeps
  /// showing done while the screen leaves.
  final bool resetAfterDone;

  /// Runs after the done state was shown (e.g. leave the screen).
  final VoidCallback? onDone;
  final Future<void> Function() onPressed;
  final String? errorFallback;
  final AppButtonVariant variant;
  final AppButtonSize size;
  final bool enabled;

  /// How long "done" shows before [onDone].
  static const doneHold = Duration(milliseconds: 650);

  @override
  State<ActionButton> createState() => _ActionButtonState();
}

/// Thrown by an [ActionButton]'s action when the member backed out: no error to show.
class ActionCancelled implements Exception {
  const ActionCancelled();
}

enum _Phase { idle, busy, done }

class _ActionButtonState extends State<ActionButton>
    with SingleTickerProviderStateMixin {
  _Phase _phase = _Phase.idle;
  String? _error;
  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 360),
  );

  @override
  void dispose() {
    _shake.dispose();
    super.dispose();
  }

  Future<void> _run() async {
    setState(() {
      _phase = _Phase.busy;
      _error = null;
    });
    try {
      await widget.onPressed();
    } on ActionCancelled {
      if (mounted) setState(() => _phase = _Phase.idle);
      return;
    } catch (e) {
      if (!mounted) return;
      final fallback = widget.errorFallback;
      setState(() {
        _phase = _Phase.idle;
        _error = fallback == null
            ? zafeErrorMessage(e)
            : zafeErrorMessage(e, fallback: fallback);
      });
      if (!MediaQuery.disableAnimationsOf(context)) {
        unawaited(_shake.forward(from: 0));
      }
      return;
    }
    if (!mounted) return;
    if (widget.doneLabel == null) {
      setState(() => _phase = _Phase.idle);
      widget.onDone?.call();
      return;
    }
    setState(() => _phase = _Phase.done);
    await Future<void>.delayed(ActionButton.doneHold);
    if (!mounted) return;
    widget.onDone?.call();
    if (widget.resetAfterDone && mounted) setState(() => _phase = _Phase.idle);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final busy = _phase == _Phase.busy;
    final done = _phase == _Phase.done;
    final label = done
        ? widget.doneLabel!
        : busy
        ? widget.busyLabel ?? widget.label
        : widget.label;
    final Widget? leading = busy
        ? AppLoadingIcon(size: 20, color: colors.icon.muted)
        : done
        ? const AppIcon(AppIcons.check, size: 20)
        : widget.icon == null
        ? null
        : AppIcon(widget.icon!, size: 20);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        AnimatedBuilder(
          animation: _shake,
          // Three damped swings, 8 px at most.
          builder: (context, child) => Transform.translate(
            offset: Offset(
              math.sin(_shake.value * math.pi * 6) * 8 * (1 - _shake.value),
              0,
            ),
            child: child,
          ),
          child: AppButton(
            expand: true,
            variant: widget.variant,
            size: widget.size,
            leading: leading == null
                ? null
                : AnimatedSwitcher(
                    duration: const Duration(milliseconds: 160),
                    transitionBuilder: (child, a) =>
                        ScaleTransition(scale: a, child: child),
                    child: KeyedSubtree(key: ValueKey(_phase), child: leading),
                  ),
            // Working and done keep the enabled look (a disabled fill would read as
            // "something's wrong"); extra taps do nothing.
            onPressed: !widget.enabled
                ? null
                : _phase == _Phase.idle
                ? _run
                : () {},
            child: Text(label),
          ),
        ),
        AnimatedSize(
          duration: const Duration(milliseconds: 180),
          curve: Curves.easeOutCubic,
          alignment: Alignment.topCenter,
          child: _error == null
              ? const SizedBox(width: double.infinity)
              : Padding(
                  padding: const EdgeInsets.only(top: AppSpacing.xs),
                  child: Semantics(
                    liveRegion: true,
                    child: Text(
                      _error!,
                      textAlign: TextAlign.center,
                      style: AppTypography.bodySmall.copyWith(
                        color: colors.text.destructive,
                      ),
                    ),
                  ),
                ),
        ),
      ],
    );
  }
}

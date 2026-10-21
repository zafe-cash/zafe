import 'dart:math' as math;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';

/// Animated loading icon used by `AppIcon(AppIcons.loader)`.
///
/// A comet arc around a faint ring (see [_LoaderIconPainter]).
class AppLoadingIcon extends StatefulWidget {
  const AppLoadingIcon({
    this.size = AppIconSize.medium,
    this.color,
    this.animated = true,
    this.semanticLabel,
    super.key,
  });

  final double size;
  final Color? color;
  final bool animated;
  final String? semanticLabel;

  @override
  State<AppLoadingIcon> createState() => _AppLoadingIconState();
}

class _AppLoadingIconState extends State<AppLoadingIcon>
    with SingleTickerProviderStateMixin {
  AnimationController? _controller;

  AnimationController get _activeController {
    return _controller ??= AnimationController(
      vsync: this,
      duration: AppLoadingIconTiming.period,
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncAnimation();
  }

  @override
  void didUpdateWidget(covariant AppLoadingIcon oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncAnimation();
  }

  bool get _effectiveAnimated {
    return widget.animated &&
        !(MediaQuery.maybeOf(context)?.disableAnimations ?? false);
  }

  void _syncAnimation() {
    if (_effectiveAnimated) {
      _activeController.repeat();
    } else {
      final controller = _controller;
      if (controller != null) {
        controller
          ..stop()
          ..value = 0;
      }
    }
  }

  @override
  void dispose() {
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _syncAnimation();
    final effectiveAnimated = _effectiveAnimated;
    final resolved =
        widget.color ??
        IconTheme.of(context).color ??
        context.colors.icon.regular;
    final icon = CustomPaint(
      painter: _LoaderIconPainter(
        color: resolved,
        repaint: effectiveAnimated ? _activeController : null,
      ),
      size: Size.square(widget.size),
    );
    final child = SizedBox.square(dimension: widget.size, child: icon);
    if (widget.semanticLabel == null) {
      return child;
    }
    return Semantics(label: widget.semanticLabel, image: true, child: child);
  }
}

abstract final class AppLoadingIconTiming {
  /// One turn of the arc.
  static const period = Duration(milliseconds: 1000);

  /// How much of the circle the arc covers (its tail fades out).
  static const sweep = 0.72;

  @visibleForTesting
  static double progressForFrameTime(Duration frameTime) {
    final frameMicros = frameTime.inMicroseconds;
    final periodMicros = period.inMicroseconds;
    return (frameMicros % periodMicros) / periodMicros;
  }
}

/// A faint ring with an arc running around it: transparent at the tail, solid at its
/// rounded head. Every loader turns in step (the frame clock, not its own start time).
/// Still: the arc stops where it is.
class _LoaderIconPainter extends CustomPainter {
  _LoaderIconPainter({required this.color, required super.repaint})
    : _animated = repaint != null,
      super();

  final Color color;
  final bool _animated;

  @override
  void paint(Canvas canvas, Size size) {
    final side = math.min(size.width, size.height);
    final stroke = math.max(1.6, side * 0.11);
    final radius = side / 2 - stroke / 2;
    final center = Offset(size.width / 2, size.height / 2);
    final rect = Rect.fromCircle(center: center, radius: radius);

    canvas.drawCircle(
      center,
      radius,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = stroke
        ..color = color.withValues(alpha: 0.14),
    );

    final turn = _animated
        ? AppLoadingIconTiming.progressForFrameTime(
            SchedulerBinding.instance.currentFrameTimeStamp,
          )
        : 0.0;
    const sweep = AppLoadingIconTiming.sweep * 2 * math.pi;
    canvas
      ..save()
      ..translate(center.dx, center.dy)
      ..rotate(turn * 2 * math.pi - math.pi / 2)
      ..translate(-center.dx, -center.dy);
    // A flat tail (a round cap there would reach past the gradient's start, where it
    // wraps to the solid colour) and a round head drawn as a dot.
    canvas.drawArc(
      rect,
      0,
      sweep,
      false,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = stroke
        ..shader = SweepGradient(
          endAngle: sweep,
          colors: [color.withValues(alpha: 0), color],
        ).createShader(rect),
    );
    canvas.drawCircle(
      center + Offset(math.cos(sweep), math.sin(sweep)) * radius,
      stroke / 2,
      Paint()..color = color,
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(covariant _LoaderIconPainter oldDelegate) {
    return oldDelegate.color != color || oldDelegate._animated != _animated;
  }
}

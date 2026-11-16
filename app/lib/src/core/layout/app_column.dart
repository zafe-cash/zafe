import 'dart:math' as math;
import 'dart:ui' show DisplayFeature;

import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';

const double kMaxContentWidth = 560;

@visibleForTesting
Rect appColumnRect(
  MediaQueryData media,
  TextDirection direction, {
  double maxWidth = kMaxContentWidth,
}) {
  final window = Offset.zero & media.size;
  final parts = DisplayFeatureSubScreen.subScreensInBounds(
    window,
    DisplayFeatureSubScreen.avoidBounds(media),
  ).toList();
  final anchor = direction == TextDirection.ltr
      ? Offset.zero
      : Offset(media.size.width, 0);
  double distance(Rect r) => (r.center - anchor).distance;
  final part = parts.isEmpty
      ? window
      : parts.reduce((a, b) => distance(b) < distance(a) ? b : a);
  final width = math.min(part.width, maxWidth);
  return Rect.fromLTWH(
    part.left + (part.width - width) / 2,
    part.top,
    width,
    part.height,
  );
}

class AppColumn extends StatelessWidget {
  const AppColumn({
    super.key,
    required this.child,
    this.maxWidth = kMaxContentWidth,
  });

  final Widget child;

  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    final rect = appColumnRect(
      media,
      Directionality.maybeOf(context) ?? TextDirection.ltr,
      maxWidth: maxWidth,
    );
    return ColoredBox(
      color: context.colors.background.window,
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          rect.left,
          rect.top,
          media.size.width - rect.right,
          media.size.height - rect.bottom,
        ),
        child: ClipRect(
          child: MediaQuery(data: columnMediaQuery(media, rect), child: child),
        ),
      ),
    );
  }
}

@visibleForTesting
MediaQueryData columnMediaQuery(MediaQueryData media, Rect rect) {
  if (rect == Offset.zero & media.size) return media;
  final inside = media.removeDisplayFeatures(rect);
  return inside.copyWith(
    size: rect.size,
    displayFeatures: [
      for (final f in inside.displayFeatures)
        DisplayFeature(
          bounds: f.bounds
              .shift(-rect.topLeft)
              .intersect(Offset.zero & rect.size),
          type: f.type,
          state: f.state,
        ),
    ],
  );
}

bool narrowerThanWindow(BuildContext context, double width) {
  final view = View.of(context);
  return width < view.physicalSize.width / view.devicePixelRatio - 1;
}

class SideFade extends StatelessWidget {
  const SideFade({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final window = context.colors.background.window;
    final clear = window.withValues(alpha: 0);
    return Stack(
      fit: StackFit.passthrough,
      children: [
        child,
        Positioned.fill(
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  stops: const [0, 0.14, 0.86, 1],
                  colors: [window, clear, clear, window],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

bool isLargeWindow(BuildContext context) {
  final view = View.of(context);
  return (view.physicalSize / view.devicePixelRatio).shortestSide >= 600;
}

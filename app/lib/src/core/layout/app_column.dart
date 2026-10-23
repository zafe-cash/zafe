import 'dart:math' as math;
import 'dart:ui' show DisplayFeature;

import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';

/// The widest a page's content gets: a phone-shaped column, centred in tablets,
/// foldables' inner screens and wide windows. Phones (at most ~430 wide) never reach it.
const double kMaxContentWidth = 560;

/// The part of the window a page uses: the start-most part not split by a hinge or a
/// half-opened fold (as Flutter's dialogs and sheets pick it, `DisplayFeatureSubScreen`),
/// at most [maxWidth] wide and centred in that part.
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
  // The same choice as dialogs and sheets (closest to the start corner), so a sheet
  // opens over the page it belongs to.
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

/// Lays [child] out in [appColumnRect] and paints the window colour around it. Below it
/// the column is the whole world: `MediaQuery` reports the column's size, the insets that
/// still reach it, and display features in its coordinates. On a phone the column is the
/// window, so nothing changes. The widget tree has the same shape at every size, so
/// resizing a window (split screen, unfolding) keeps the page's state.
class AppColumn extends StatelessWidget {
  const AppColumn({
    super.key,
    required this.child,
    this.maxWidth = kMaxContentWidth,
  });

  final Widget child;

  /// [double.infinity] keeps only the hinge rule (a page that lays out wide windows
  /// itself, e.g. Welcome).
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
        // Clipped like a phone's screen edge: things drawn past it (the sync status
        // glow, a coin burst) stop there.
        child: ClipRect(
          child: MediaQuery(data: columnMediaQuery(media, rect), child: child),
        ),
      ),
    );
  }
}

/// [media] as seen from inside [rect]: its size, the insets that reach it, and the
/// display features crossing it, moved to its coordinates.
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

/// Whether a picture [width] wide leaves part of the window beside it (in a column on a
/// tablet, say): then its sides should fade into the window colour, not end in an edge.
bool narrowerThanWindow(BuildContext context, double width) {
  final view = View.of(context);
  return width < view.physicalSize.width / view.devicePixelRatio - 1;
}

/// Fades [child]'s left and right edges into the window colour (art drawn narrower than
/// the window, see [narrowerThanWindow]).
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

/// A tablet or an unfolded foldable (the window's shortest side is at least 600, the
/// line Android draws for large screens), as opposed to a phone or a part of a split
/// screen. Layouts that put free space at the bottom on a phone centre it here.
bool isLargeWindow(BuildContext context) {
  final view = View.of(context);
  return (view.physicalSize / view.devicePixelRatio).shortestSide >= 600;
}

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Displays at least this many logical pixels on their shortest side rotate: tablets,
/// foldables' inner screens. The same line as Android's large screens (sw600), where
/// Android 16 ignores an app's orientation lock anyway.
const double kRotatingShortestSide = 600;

/// Whether the app may rotate. Phones (also a foldable's cover screen and a flip phone's
/// main screen) stay portrait: every screen is laid out for a phone held upright first.
/// Null while neither size is known yet (nothing to decide).
///
/// [display] is the whole screen (Android: the largest window the display allows, kept
/// current through folding; iOS: read once at start), [window] the app's window. The
/// larger shortest side wins, so a split-screen window on a tablet doesn't lock it, and
/// unfolding a foldable whose display iOS reported at start still counts.
@visibleForTesting
bool? allowsRotation({required Size display, required Size window}) {
  final shortest = math.max(display.shortestSide, window.shortestSide);
  if (shortest <= 0) return null;
  return shortest >= kRotatingShortestSide;
}

/// Applies [allowsRotation] at start and again whenever the window or display changes
/// (rotating, resizing, unfolding: Android keeps the activity through these, see
/// `configChanges` in the manifest). The platform files allow every orientation; this
/// narrows phones to portrait.
class OrientationPolicy with WidgetsBindingObserver {
  OrientationPolicy._();

  static OrientationPolicy? _instance;
  bool? _applied;

  /// Call once, after `WidgetsFlutterBinding.ensureInitialized()`.
  static void install() {
    if (_instance != null) return;
    final policy = _instance = OrientationPolicy._();
    WidgetsBinding.instance.addObserver(policy);
    policy._apply();
  }

  @override
  void didChangeMetrics() => _apply();

  void _apply() {
    final view = WidgetsBinding.instance.platformDispatcher.implicitView;
    if (view == null) return;
    final rotate = allowsRotation(
      display: _displaySize(view),
      window: view.physicalSize / view.devicePixelRatio,
    );
    if (rotate == null || rotate == _applied) return;
    _applied = rotate;
    unawaited(
      SystemChrome.setPreferredOrientations(
        // Empty = whatever the platform allows (Android: unspecified, so the user's
        // rotation lock still applies).
        rotate ? const [] : const [DeviceOrientation.portraitUp],
      ),
    );
  }

  static Size _displaySize(ui.FlutterView view) {
    try {
      final display = view.display;
      return display.size / display.devicePixelRatio;
    } catch (_) {
      // No display reported yet.
      return Size.zero;
    }
  }
}

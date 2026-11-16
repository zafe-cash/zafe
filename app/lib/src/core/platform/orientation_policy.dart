import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

const double kRotatingShortestSide = 600;

@visibleForTesting
bool? allowsRotation({required Size display, required Size window}) {
  final shortest = math.max(display.shortestSide, window.shortestSide);
  if (shortest <= 0) return null;
  return shortest >= kRotatingShortestSide;
}

class OrientationPolicy with WidgetsBindingObserver {
  OrientationPolicy._();

  static OrientationPolicy? _instance;
  bool? _applied;

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
        rotate ? const [] : const [DeviceOrientation.portraitUp],
      ),
    );
  }

  static Size _displaySize(ui.FlutterView view) {
    try {
      final display = view.display;
      return display.size / display.devicePixelRatio;
    } catch (_) {
      return Size.zero;
    }
  }
}

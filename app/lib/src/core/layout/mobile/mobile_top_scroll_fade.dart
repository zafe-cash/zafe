import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';

class MobileTopScrollFade extends StatefulWidget {
  const MobileTopScrollFade({
    required this.child,
    this.height = AppSpacing.base,
    super.key,
  });

  final Widget child;

  final double height;

  @override
  State<MobileTopScrollFade> createState() => _MobileTopScrollFadeState();
}

class _MobileTopScrollFadeState extends State<MobileTopScrollFade> {
  double _progress = 0;

  static const _activationDistance = AppSpacing.md;

  static const _stops = [
    0.0,
    0.071,
    0.143,
    0.214,
    0.286,
    0.357,
    0.429,
    0.5,
    0.571,
    0.643,
    0.714,
    0.786,
    0.857,
    0.929,
    1.0,
  ];
  static const _alphas = [
    1.0,
    0.985,
    0.945,
    0.882,
    0.802,
    0.708,
    0.606,
    0.5,
    0.394,
    0.292,
    0.198,
    0.118,
    0.055,
    0.015,
    0.0,
  ];

  bool _onScroll(ScrollNotification notification) {
    if (notification.depth != 0) return false;
    final progress = (notification.metrics.extentBefore / _activationDistance)
        .clamp(0.0, 1.0);
    final next = _smoothstep(progress);
    if (next != _progress) setState(() => _progress = next);
    return false;
  }

  static double _smoothstep(double value) => value * value * (3 - 2 * value);

  @override
  Widget build(BuildContext context) {
    final window = context.colors.background.window;
    final fadeHeight = widget.height * _progress;
    return NotificationListener<ScrollNotification>(
      onNotification: _onScroll,
      child: Stack(
        children: [
          widget.child,
          Positioned(
            top: 0,
            left: 0,
            right: 0,
            height: fadeHeight,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    stops: _stops,
                    colors: [
                      for (var i = 0; i < _stops.length; i++)
                        window.withValues(alpha: _gradientAlpha(i)),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  double _gradientAlpha(int stopIndex) =>
      stopIndex == 0 ? _alphas[stopIndex] : _alphas[stopIndex] * _progress;
}

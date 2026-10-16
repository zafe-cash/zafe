import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// The Welcome door's wheel centre: `WELCOME_WHEEL` in scripts/illustrations/scenes.py.
const _wheel = Offset(540, 560);

/// The Welcome vault door, drawn from its layers (scenes.py `LAYERS`): every few seconds
/// its hand wheel turns a little, as if someone were trying it, and springs back. Short
/// moments, not a loop: between them nothing animates (a running animation redraws the
/// whole screen every frame). Paused while another screen covers it; reduced motion
/// keeps it still.
class WelcomeArt extends StatefulWidget {
  const WelcomeArt({super.key});

  @override
  State<WelcomeArt> createState() => _WelcomeArtState();
}

class _WelcomeArtState extends State<WelcomeArt>
    with SingleTickerProviderStateMixin {
  static const _every = Duration(seconds: 7);

  late final AnimationController _turn = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1700),
  );

  /// Degrees over the turn: a slow quarter-ish pull, a beat, then back with a
  /// little overshoot, like a heavy wheel settling.
  late final Animation<double> _angle = TweenSequence<double>([
    TweenSequenceItem(
      tween: Tween(
        begin: 0.0,
        end: -32.0,
      ).chain(CurveTween(curve: Curves.easeInOutCubic)),
      weight: 40,
    ),
    TweenSequenceItem(tween: ConstantTween(-32.0), weight: 12),
    TweenSequenceItem(
      tween: Tween(
        begin: -32.0,
        end: 0.0,
      ).chain(CurveTween(curve: Curves.easeOutBack)),
      weight: 48,
    ),
  ]).animate(_turn);

  Timer? _timer;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _timer?.cancel();
    if (MediaQuery.disableAnimationsOf(context)) {
      _turn.value = 0;
      return;
    }
    // The first turn soon after the screen opens, then every few seconds. A covered
    // screen's ticker is muted, so a turn there doesn't draw.
    _timer = Timer(const Duration(milliseconds: 1400), () {
      _play();
      _timer = Timer.periodic(_every, (_) => _play());
    });
  }

  void _play() {
    if (mounted && TickerMode.valuesOf(context).enabled) _turn.forward(from: 0);
  }

  @override
  void dispose() {
    _timer?.cancel();
    _turn.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/welcome_vault_${name}_${dark ? 'dark' : 'light'}.svg';
  }

  @override
  Widget build(BuildContext context) {
    Widget layer(String name) => RepaintBoundary(
      child: SvgPicture.asset(
        _layer(context, name),
        width: 1080,
        height: 1240,
        fit: BoxFit.fill,
      ),
    );
    final wheel = layer('wheel');
    return FittedBox(
      fit: BoxFit.cover,
      child: SizedBox(
        width: 1080,
        height: 1240,
        child: Stack(
          children: [
            layer('bg'),
            AnimatedBuilder(
              animation: _angle,
              builder: (context, child) => Transform(
                transform: Matrix4.identity()
                  ..translateByDouble(_wheel.dx, _wheel.dy, 0, 1)
                  ..rotateZ(_angle.value * math.pi / 180)
                  ..translateByDouble(-_wheel.dx, -_wheel.dy, 0, 1),
                child: child,
              ),
              child: wheel,
            ),
            layer('glint'),
            layer('top'),
          ],
        ),
      ),
    );
  }
}

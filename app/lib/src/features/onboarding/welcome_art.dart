import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// The Welcome door's wheel centre and key ring: `WELCOME_WHEEL` and the scene's `ring`
/// in scripts/illustrations/scenes.py.
const _centre = Offset(540, 560);
const _ring = 225.0;

/// The keys that turn (scene `turned`), in the order they turn: upper-left, top,
/// upper-right. Key k sits at -90 + 72k degrees on the ring.
const _keys = [4, 0, 1];

Offset _keyAt(int k) {
  final a = (-90 + 72 * k) * math.pi / 180;
  return _centre + Offset(math.cos(a), math.sin(a)) * _ring;
}

/// The Welcome vault door, from its layers (scenes.py `LAYERS`), telling what Zafe is:
/// three of five keys turn one after another, the gold arc joining them grows, the wheel
/// spins a full turn, the bolts slide back and a gold ripple says it's open. Unlocked, it holds, quietly locks again
/// and repeats. Paused while another screen covers it; reduced motion shows it open.
class WelcomeArt extends StatefulWidget {
  const WelcomeArt({super.key});

  @override
  State<WelcomeArt> createState() => _WelcomeArtState();
}

class _WelcomeArtState extends State<WelcomeArt>
    with SingleTickerProviderStateMixin {
  static const _unlock = 3.1; // seconds

  late final AnimationController _open = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 3100),
    reverseDuration: const Duration(milliseconds: 900),
  );

  /// Locking again plays the unlock backwards, faster, except the wheel: five spokes
  /// look the same after a full turn, so it simply stays.
  bool _locking = false;
  bool _looping = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _open.value = 1;
    } else if (!_looping) {
      _looping = true;
      unawaited(_loop());
    }
  }

  /// The next step's timer, cancelled with the screen (pending waits would outlive it).
  Timer? _wait;

  void _after(int ms, VoidCallback next) {
    _wait?.cancel();
    _wait = Timer(Duration(milliseconds: ms), () {
      if (mounted) next();
    });
  }

  /// Unlock, hold, lock again, pause, and over.
  Future<void> _loop() async {
    _after(700, () async {
      _locking = false;
      await _open.forward(from: 0).orCancel.catchError((_) {});
      if (!mounted) return;
      _after(4500, () async {
        _locking = true;
        await _open.reverse().orCancel.catchError((_) {});
        if (mounted) _after(600, _loop);
      });
    });
  }

  @override
  void dispose() {
    _wait?.cancel();
    _open.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/welcome_vault_${name}_${dark ? 'dark' : 'light'}.svg';
  }

  /// 0-1 progress of a step that runs from `from` for `length` seconds of the unlock.
  double _step(
    double from,
    double length, [
    Curve curve = Curves.easeInOutCubic,
  ]) {
    final s = _open.value * _unlock;
    return curve.transform(((s - from) / length).clamp(0.0, 1.0));
  }

  static Matrix4 _about(Offset c, double radians) => Matrix4.identity()
    ..translateByDouble(c.dx, c.dy, 0, 1)
    ..rotateZ(radians)
    ..translateByDouble(-c.dx, -c.dy, 0, 1);

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
    final bg = layer('bg'), door = layer('door'), locks = layer('locks');
    final boltsL = layer('boltsL'), boltsR = layer('boltsR');
    final wheel = layer('wheel'), glint = layer('glint'), top = layer('top');
    final glows = {for (final k in _keys) k: layer('glow$k')};
    final keys = {for (final k in _keys) k: layer('key$k')};
    return FittedBox(
      fit: BoxFit.cover,
      child: SizedBox(
        width: 1080,
        height: 1240,
        child: AnimatedBuilder(
          animation: _open,
          builder: (context, _) {
            // Each key turns a quarter into place, one after another, lighting up.
            final turns = [
              for (var i = 0; i < _keys.length; i++)
                _step(0.15 + i * 0.35, 0.45, Curves.easeOutBack),
            ];
            final arc = _step(0.25, 1.1);
            final spin = _locking ? 0.0 : _step(1.35, 1.1);
            final bolts = _step(2.1, 0.5) * 34;
            // Unlocked: a gold ripple from the key ring, once.
            final click = _locking ? 0.0 : _step(2.5, 0.6, Curves.easeOutCubic);
            return Stack(
              children: [
                bg,
                Transform.translate(offset: Offset(bolts, 0), child: boltsL),
                Transform.translate(offset: Offset(-bolts, 0), child: boltsR),
                door,
                CustomPaint(
                  size: const Size(1080, 1240),
                  painter: _ArcPainter(arc, click),
                ),
                for (var i = 0; i < _keys.length; i++)
                  Opacity(
                    opacity: turns[i].clamp(0.0, 1.0),
                    child: glows[_keys[i]],
                  ),
                locks,
                for (var i = 0; i < _keys.length; i++)
                  Transform(
                    transform: _about(
                      _keyAt(_keys[i]),
                      -(1 - turns[i]) * math.pi / 2,
                    ),
                    child: keys[_keys[i]],
                  ),
                Transform(
                  transform: _about(_centre, spin * 2 * math.pi),
                  child: wheel,
                ),
                glint,
                top,
              ],
            );
          },
        ),
      ),
    );
  }
}

/// The gold arc joining the turned keys, grown to `progress` (scene: from the upper-left
/// key over the top to the upper-right one, width 10, 90%).
class _ArcPainter extends CustomPainter {
  _ArcPainter(this.progress, this.click);
  final double progress;

  /// 0-1 through the "unlocked" ripple (0 and 1: none).
  final double click;

  @override
  void paint(Canvas canvas, Size size) {
    if (click > 0 && click < 1) {
      canvas.drawCircle(
        _centre,
        _ring + 120 * click,
        Paint()
          ..color = const Color(
            0xFFF4B728,
          ).withValues(alpha: 0.85 * (1 - click))
          ..style = PaintingStyle.stroke
          ..strokeWidth = 14 * (1 - click) + 2,
      );
    }
    if (progress <= 0) return;
    const start = 198 * math.pi / 180;
    const sweep = 144 * math.pi / 180;
    canvas.drawArc(
      Rect.fromCircle(center: _centre, radius: _ring),
      start,
      sweep * progress,
      false,
      Paint()
        ..color = const Color(0xFFF4B728).withValues(alpha: 0.9)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 10
        ..strokeCap = StrokeCap.round,
    );
  }

  @override
  bool shouldRepaint(_ArcPainter old) =>
      old.progress != progress || old.click != click;
}

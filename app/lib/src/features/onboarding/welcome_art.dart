import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// The Welcome door's wheel centre and key ring: `WELCOME_WHEEL` and the scene's `ring`
/// in scripts/illustrations/scenes.py.
const _centre = Offset(540, 560);
const _ring = 225.0;
const _gold = Color(0xFFF4B728);

/// The keys that turn (scene `turned`), in the order they turn: upper-left, top,
/// upper-right. Key k sits at -90 + 72k degrees on the ring.
const _keys = [4, 0, 1];

/// The gold arc joining them: from the upper-left key over the top to the upper-right.
const _arcStart = 198 * math.pi / 180;
const _arcSweep = 144 * math.pi / 180;

Offset _keyAt(int k) {
  final a = (-90 + 72 * k) * math.pi / 180;
  return _centre + Offset(math.cos(a), math.sin(a)) * _ring;
}

/// A small move the other way first (anticipation), then the move with an overshoot
/// that settles: how a key or a heavy wheel turns by hand.
class _WindUp extends Curve {
  const _WindUp(this.back, this.overshoot, {this.windUp = 0.2});
  final double back; // fraction of the move, the other way
  final double overshoot; // easeOutBack's tension
  final double windUp; // share of the time spent winding up

  @override
  double transformInternal(double t) {
    if (t < windUp) {
      return -back * math.sin(t / windUp * math.pi / 2);
    }
    final u = (t - windUp) / (1 - windUp);
    final s = overshoot;
    final p = u - 1;
    final out = p * p * ((s + 1) * p + s) + 1; // easeOutBack with tension s
    return -back + (1 + back) * out;
  }
}

/// The Welcome vault door, from its layers (scenes.py `LAYERS`), telling what Zafe is:
/// three of five keys turn, each with a little wind-up, the gold arc joining them
/// grows, the heavy wheel spins a full turn, the bolts slide back and the door settles
/// with a gold ripple. It plays once when the screen appears (and when it comes back);
/// tapping the door plays it again, with a haptic tick per key. Once open, the gold
/// swells with light and settles, over and over. Reduced motion: the door open, still.
class WelcomeArt extends StatefulWidget {
  const WelcomeArt({super.key, this.replay});

  /// Plays the unlock again (with haptic ticks) each time it notifies: the screen's
  /// tap target over the door (the art itself sits under the screen's scroll view).
  final Listenable? replay;

  @override
  State<WelcomeArt> createState() => _WelcomeArtState();
}

class _WelcomeArtState extends State<WelcomeArt> with TickerProviderStateMixin {
  static const _unlock = 3.2; // seconds

  late final AnimationController _open = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 3200),
    reverseDuration: const Duration(milliseconds: 500),
  );

  /// Once open: the gold swells with light in place and settles, over and over
  /// (Vizor's welcome hero does the same over its treasure: ~2.3 s of light every
  /// 3.8 s). One cycle: 0.9 s up, 0.8 s held, 0.6 s down, 2.3 s rest.
  late final AnimationController _shine = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 4600),
  );

  /// 0-1 light of the bloom now.
  double get _bloom {
    final s = _shine.value * 4.6;
    if (s < 0.9) return Curves.easeOutSine.transform(s / 0.9);
    if (s < 1.7) return 1;
    if (s < 2.3) return 1 - Curves.easeInSine.transform((s - 1.7) / 0.6);
    return 0;
  }

  Timer? _startTimer;

  /// Locking again (before a replay) runs the unlock backwards, quickly, except the
  /// wheel: five spokes look the same after a full turn, so it stays.
  bool _locking = false;
  bool _visible = false;
  bool _haptics = false;
  final _ticked = <int>{};

  bool get _still => MediaQuery.disableAnimationsOf(context);

  @override
  void initState() {
    super.initState();
    _open.addListener(_hapticTicks);
    widget.replay?.addListener(_replay);
  }

  void _replay() {
    if (!_still) unawaited(_play(haptics: true));
  }

  @override
  void didUpdateWidget(WelcomeArt old) {
    super.didUpdateWidget(old);
    if (old.replay != widget.replay) {
      old.replay?.removeListener(_replay);
      widget.replay?.addListener(_replay);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_still) {
      _open.value = 1;
      return;
    }
    // Visible again (first build, or back from a screen on top): play it.
    final visible = TickerMode.valuesOf(context).enabled;
    if (visible && !_visible) {
      _startTimer?.cancel();
      _startTimer = Timer(const Duration(milliseconds: 600), () => _play());
    }
    _visible = visible;
  }

  Future<void> _play({bool haptics = false}) async {
    if (!mounted || _open.isAnimating) return;
    _shine.stop();
    _shine.value = 0;
    if (_open.value > 0) {
      _locking = true;
      await _open.reverse().orCancel.catchError((_) {});
      if (!mounted) return;
    }
    _locking = false;
    _haptics = haptics;
    _ticked.clear();
    await _open.forward(from: 0).orCancel.catchError((_) {});
    if (!mounted) return;
    // A covered screen's ticker is muted, so the repeat costs nothing there.
    unawaited(_shine.repeat());
  }

  /// When replayed by a tap: a light tick as each key turns, a firmer one as the bolts
  /// clear. Never when it plays by itself (no haptics without a touch).
  void _hapticTicks() {
    if (!_haptics || _locking) return;
    final s = _open.value * _unlock;
    for (var i = 0; i < _keys.length; i++) {
      if (s >= _keyStart(i) + 0.2 && _ticked.add(i)) {
        unawaited(HapticFeedback.selectionClick());
      }
    }
    if (s >= 2.55 && _ticked.add(99)) unawaited(HapticFeedback.lightImpact());
  }

  static double _keyStart(int i) => 0.1 + i * 0.32;

  @override
  void dispose() {
    widget.replay?.removeListener(_replay);
    _startTimer?.cancel();
    _open.dispose();
    _shine.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/welcome_vault_${name}_${dark ? 'dark' : 'light'}.svg';
  }

  /// Progress of a step running from `from` for `length` seconds of the unlock.
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
          animation: Listenable.merge([_open, _shine]),
          builder: (context, _) {
            const turn = _WindUp(0.09, 2.2);
            final turns = [
              for (var i = 0; i < _keys.length; i++)
                _step(_keyStart(i), 0.55, turn),
            ];
            final arc = _step(0.3, 1.05);
            // The heavy wheel: winds back a little, spins, overshoots a few degrees.
            final spin = _locking
                ? 0.0
                : _step(1.4, 1.15, const _WindUp(0.035, 0.6, windUp: 0.22));
            // The bolts snap back once the wheel has nearly stopped.
            final bolts = _step(2.25, 0.32, Curves.easeInCubic) * 34;
            // Unlocked: the door settles with a small shake and a gold ripple.
            final settle = _locking ? 0.0 : _step(2.55, 0.45, Curves.linear);
            final shake = settle <= 0 || settle >= 1
                ? 0.0
                : math.sin(settle * math.pi * 3) * 4 * (1 - settle);
            final click = _locking
                ? 0.0
                : _step(2.55, 0.65, Curves.easeOutCubic);
            final face = Offset(0, shake);
            return Stack(
              children: [
                bg,
                Transform.translate(offset: Offset(bolts, 0), child: boltsL),
                Transform.translate(offset: Offset(-bolts, 0), child: boltsR),
                Transform.translate(
                  offset: face,
                  child: Stack(
                    children: [
                      door,
                      CustomPaint(
                        size: const Size(1080, 1240),
                        painter: _ArcPainter(arc, click, _bloom),
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
                      CustomPaint(
                        size: const Size(1080, 1240),
                        painter: _BloomPainter(
                          _bloom,
                          identical(context.colors, AppColors.dark),
                        ),
                      ),
                    ],
                  ),
                ),
                top,
              ],
            );
          },
        ),
      ),
    );
  }
}

/// A see-through tap target over the Welcome door (the hero's 1080 x 1240 canvas,
/// door centre 540,560, radius ~395): taps replay the unlock, drags still reach the
/// scroll view underneath.
class WelcomeDoorTap extends StatelessWidget {
  const WelcomeDoorTap({super.key, required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Positioned(
    top: 0,
    left: 0,
    right: 0,
    child: AspectRatio(
      aspectRatio: 1080 / 1240,
      child: Align(
        alignment: Alignment(0, 560 / 1240 * 2 - 1),
        child: FractionallySizedBox(
          widthFactor: 790 / 1080,
          heightFactor: 790 / 1240,
          child: ExcludeSemantics(
            child: GestureDetector(
              behavior: HitTestBehavior.translucent,
              onTap: onTap,
            ),
          ),
        ),
      ),
    ),
  );
}

/// The gold arc joining the turned keys grown to `progress`, the "unlocked" ripple at
/// `click` (0 and 1: none), and the arc warming with the bloom at `shine` (0-1).
class _ArcPainter extends CustomPainter {
  _ArcPainter(this.progress, this.click, this.shine);
  final double progress, click, shine;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Rect.fromCircle(center: _centre, radius: _ring);
    if (click > 0 && click < 1) {
      canvas.drawCircle(
        _centre,
        _ring + 130 * click,
        Paint()
          ..color = _gold.withValues(alpha: 0.85 * (1 - click))
          ..style = PaintingStyle.stroke
          ..strokeWidth = 14 * (1 - click) + 2,
      );
    }
    if (progress <= 0) return;
    canvas.drawArc(
      rect,
      _arcStart,
      _arcSweep * progress,
      false,
      Paint()
        ..color = _gold.withValues(alpha: 0.9)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 10
        ..strokeCap = StrokeCap.round,
    );
    if (shine > 0 && progress >= 1) {
      // The whole arc warms with the bloom, in place.
      canvas.drawArc(
        rect,
        _arcStart,
        _arcSweep,
        false,
        Paint()
          ..color = const Color(0xFFFFF0C2).withValues(alpha: 0.5 * shine)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 8
          ..strokeCap = StrokeCap.round
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 2),
      );
    }
  }

  @override
  bool shouldRepaint(_ArcPainter old) =>
      old.progress != progress || old.click != click || old.shine != shine;
}

/// The bloom: warm light swelling over the gold, around the three keys and the arc
/// (added light, so the gold and the metal under it brighten), at `light` 0-1.
class _BloomPainter extends CustomPainter {
  _BloomPainter(this.light, this.dark);
  final double light;
  final bool dark;

  @override
  void paint(Canvas canvas, Size size) {
    if (light <= 0) return;
    // Between Zafe's faint gleam and Vizor's chest glow (21% brighter at its peak).
    // Measured: 0.16 brightened the upper door 30% at its peak; this is ~15%.
    final peak = (dark ? 0.08 : 0.12) * light;
    final glow = Paint()..blendMode = BlendMode.plus;
    void bloomAt(Offset c, double r, double a) {
      glow.shader = ui.Gradient.radial(c, r, [
        const Color(0xFFFFE7A8).withValues(alpha: a),
        const Color(0x00FFE7A8),
      ]);
      canvas.drawCircle(c, r, glow);
    }

    // Over the upper door, where the arc runs, and a brighter heart at each key.
    bloomAt(_centre - const Offset(0, 120), 360, peak * 0.7);
    for (final k in _keys) {
      bloomAt(_keyAt(k), 140, peak);
    }
  }

  @override
  bool shouldRepaint(_BloomPainter old) =>
      old.light != light || old.dark != dark;
}

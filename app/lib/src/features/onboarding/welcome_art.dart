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

/// The Welcome vault door, from its layers (scenes.py `LAYERS`). The still already tells
/// the story (three of five keys turned, the gold arc joining them), so only the heavy
/// hand wheel moves: it is worked by hand, in uneven pushes (20-50 degrees, a little
/// give-back as the hand lets go, pauses of uneven length), never quite the same rhythm.
/// Now and then the gold blooms with light in place (as Vizor's welcome hero does over
/// its treasure). Tapping the door gives the wheel one strong full turn, with a haptic
/// tick. Nothing is drawn between moves. Reduced motion: still.
class WelcomeArt extends StatefulWidget {
  const WelcomeArt({super.key, this.replay});

  /// Gives the wheel a full turn (with haptics) each time it notifies: the screen's tap
  /// target over the door (the art itself sits under the screen's scroll view).
  final Listenable? replay;

  @override
  State<WelcomeArt> createState() => _WelcomeArtState();
}

class _WelcomeArtState extends State<WelcomeArt> with TickerProviderStateMixin {
  /// One push of the wheel: from [_from] by [_by] degrees.
  late final AnimationController _push = AnimationController(vsync: this);
  double _from = 0, _by = 0;

  /// The gold's light: 0.9 s up, 0.8 s held, 0.6 s down; then a rest.
  late final AnimationController _shine = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 2300),
  );

  Timer? _nextPush, _nextBloom;
  final _rng = math.Random(7);
  bool _visible = false;

  bool get _still => MediaQuery.disableAnimationsOf(context);

  double get _wheelDegrees {
    final u = _push.value;
    // The hand pushes (ease in and out) a touch past, then the wheel gives back.
    const give = 0.07;
    if (u < 0.82) {
      return _from +
          _by * (1 + give) * Curves.easeInOutCubic.transform(u / 0.82);
    }
    final back = Curves.easeOutSine.transform(
      ((u - 0.82) / 0.18).clamp(0.0, 1.0),
    );
    return _from + _by * (1 + give - give * back);
  }

  double get _bloom {
    final s = _shine.value * 2.3;
    if (s <= 0 || s >= 2.3) return 0;
    if (s < 0.9) return Curves.easeOutSine.transform(s / 0.9);
    if (s < 1.7) return 1;
    return 1 - Curves.easeInSine.transform(((s - 1.7) / 0.6).clamp(0.0, 1.0));
  }

  @override
  void initState() {
    super.initState();
    widget.replay?.addListener(_fullTurn);
  }

  @override
  void didUpdateWidget(WelcomeArt old) {
    super.didUpdateWidget(old);
    if (old.replay != widget.replay) {
      old.replay?.removeListener(_fullTurn);
      widget.replay?.addListener(_fullTurn);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final visible = !_still && TickerMode.valuesOf(context).enabled;
    if (visible && !_visible) {
      _schedulePush(const Duration(milliseconds: 700));
      _scheduleBloom(const Duration(milliseconds: 2600));
    } else if (!visible) {
      _nextPush?.cancel();
      _nextBloom?.cancel();
    }
    _visible = visible;
  }

  void _schedulePush(Duration after) {
    _nextPush?.cancel();
    _nextPush = Timer(after, () => _turn());
  }

  void _scheduleBloom(Duration after) {
    _nextBloom?.cancel();
    _nextBloom = Timer(after, () async {
      if (!mounted) return;
      await _shine.forward(from: 0).orCancel.catchError((_) {});
      // Rarer than Vizor's (every 3.8 s): the wheel already gives the door life.
      if (mounted && _visible) {
        _scheduleBloom(const Duration(milliseconds: 5200));
      }
    });
  }

  /// One uneven push by hand, then a pause of uneven length before the next.
  Future<void> _turn({double? by, Duration? over}) async {
    if (!mounted) return;
    _from = _wheelDegrees % 360;
    _by = by ?? 20 + _rng.nextDouble() * 30;
    _push.duration =
        over ?? Duration(milliseconds: 600 + (_rng.nextDouble() * 500).round());
    await _push.forward(from: 0).orCancel.catchError((_) {});
    if (!mounted || !_visible) return;
    // Now and then two pushes close together, like re-gripping.
    final pause = _rng.nextDouble() < 0.3
        ? 250 + _rng.nextInt(250)
        : 700 + _rng.nextInt(1300);
    _schedulePush(Duration(milliseconds: pause));
  }

  /// A tap: one strong full turn, a tick as the hand takes it and a firmer one as it
  /// lands.
  Future<void> _fullTurn() async {
    if (_still || _push.isAnimating) return;
    _nextPush?.cancel();
    unawaited(HapticFeedback.selectionClick());
    await _turn(by: 360, over: const Duration(milliseconds: 1500));
    if (mounted) unawaited(HapticFeedback.lightImpact());
  }

  @override
  void dispose() {
    widget.replay?.removeListener(_fullTurn);
    _nextPush?.cancel();
    _nextBloom?.cancel();
    _push.dispose();
    _shine.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/welcome_vault_${name}_${dark ? 'dark' : 'light'}.svg';
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
    final dark = identical(context.colors, AppColors.dark);
    final still = [
      layer('bg'),
      layer('boltsL'),
      layer('boltsR'),
      layer('door'),
    ];
    final over = [
      for (final k in _keys) layer('glow$k'),
      layer('locks'),
      for (final k in _keys) layer('key$k'),
    ];
    // The rim and its light stay still; the spokes, grips and hub turn above them (the
    // light drawn over the turning grips cut across them).
    final rim = layer('rim'), glint = layer('glint');
    final wheel = layer('wheel'), top = layer('top');
    return FittedBox(
      fit: BoxFit.cover,
      child: SizedBox(
        width: 1080,
        height: 1240,
        child: Stack(
          children: [
            ...still,
            AnimatedBuilder(
              animation: _shine,
              builder: (context, _) => CustomPaint(
                size: const Size(1080, 1240),
                painter: _ArcPainter(_bloom),
              ),
            ),
            ...over,
            rim,
            glint,
            AnimatedBuilder(
              animation: _push,
              builder: (context, child) => Transform(
                transform: _about(_centre, _wheelDegrees * math.pi / 180),
                child: child,
              ),
              child: wheel,
            ),
            AnimatedBuilder(
              animation: _shine,
              builder: (context, _) => CustomPaint(
                size: const Size(1080, 1240),
                painter: _BloomPainter(_bloom, dark),
              ),
            ),
            top,
          ],
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

/// The gold arc joining the turned keys (scene: from the upper-left key over the top
/// to the upper-right one, width 10, 90%), warming with the bloom at `shine` (0-1).
class _ArcPainter extends CustomPainter {
  _ArcPainter(this.shine);
  final double shine;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Rect.fromCircle(center: _centre, radius: _ring);
    canvas.drawArc(
      rect,
      _arcStart,
      _arcSweep,
      false,
      Paint()
        ..color = _gold.withValues(alpha: 0.9)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 10
        ..strokeCap = StrokeCap.round,
    );
    if (shine > 0) {
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
  bool shouldRepaint(_ArcPainter old) => old.shine != shine;
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

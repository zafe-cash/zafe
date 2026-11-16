import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';
import 'onboarding_art.dart';

const _centre = Offset(540, 560);
const _ring = 225.0;
const _gold = Color(0xFFF4B728);

const _keys = [4, 0, 1];

const _arcStart = 198 * math.pi / 180;
const _arcSweep = 144 * math.pi / 180;

Offset _keyAt(int k) {
  final a = (-90 + 72 * k) * math.pi / 180;
  return _centre + Offset(math.cos(a), math.sin(a)) * _ring;
}

class WelcomeArt extends StatefulWidget {
  const WelcomeArt({super.key, this.replay});

  final Listenable? replay;

  @override
  State<WelcomeArt> createState() => _WelcomeArtState();
}

class _WelcomeArtState extends State<WelcomeArt> with TickerProviderStateMixin {
  late final AnimationController _push = AnimationController(vsync: this);
  double _from = 0, _by = 0;

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
      if (mounted && _visible) {
        _scheduleBloom(const Duration(milliseconds: 5200));
      }
    });
  }

  Future<void> _turn({double? by, Duration? over}) async {
    if (!mounted) return;
    _from = _wheelDegrees % 360;
    _by = by ?? 20 + _rng.nextDouble() * 30;
    _push.duration =
        over ?? Duration(milliseconds: 600 + (_rng.nextDouble() * 500).round());
    await _push.forward(from: 0).orCancel.catchError((_) {});
    if (!mounted || !_visible) return;
    final pause = _rng.nextDouble() < 0.3
        ? 250 + _rng.nextInt(250)
        : 700 + _rng.nextInt(1300);
    _schedulePush(Duration(milliseconds: pause));
  }

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

class WelcomeDoorTap extends StatelessWidget {
  const WelcomeDoorTap({super.key, required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Positioned(
    top: 0,
    left: 0,
    right: 0,
    child: Align(
      alignment: Alignment.topCenter,
      child: SizedBox.fromSize(
        size: OnboardingHero.sizeIn(MediaQuery.sizeOf(context)),
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
    ),
  );
}

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

class _BloomPainter extends CustomPainter {
  _BloomPainter(this.light, this.dark);
  final double light;
  final bool dark;

  @override
  void paint(Canvas canvas, Size size) {
    if (light <= 0) return;
    final peak = (dark ? 0.08 : 0.12) * light;
    final glow = Paint()..blendMode = BlendMode.plus;
    void bloomAt(Offset c, double r, double a) {
      glow.shader = ui.Gradient.radial(c, r, [
        const Color(0xFFFFE7A8).withValues(alpha: a),
        const Color(0x00FFE7A8),
      ]);
      canvas.drawCircle(c, r, glow);
    }

    bloomAt(_centre - const Offset(0, 120), 360, peak * 0.7);
    for (final k in _keys) {
      bloomAt(_keyAt(k), 140, peak);
    }
  }

  @override
  bool shouldRepaint(_BloomPainter old) =>
      old.light != light || old.dark != dark;
}

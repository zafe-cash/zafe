import 'dart:math' as math;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

const _trails = [
  [Offset(480, 320), Offset(250, 560), Offset(70, 1000)],
  [Offset(600, 320), Offset(840, 520), Offset(1010, 940)],
];

Offset _on(List<Offset> t, double u) =>
    t[0] * ((1 - u) * (1 - u)) + t[1] * (2 * (1 - u) * u) + t[2] * (u * u);

double _rand(double seed) {
  final w = math.sin(seed * 12.9898) * 43758.5453;
  return w - w.floorToDouble();
}

String _coinAsset(BuildContext context) {
  final dark = identical(context.colors, AppColors.dark);
  return 'assets/illustrations/layers/sent_slot_coin_${dark ? 'dark' : 'light'}.svg';
}

Widget _placedCoin(
  Widget coin,
  Offset pos,
  double size,
  double flip,
  double turn,
  double opacity, {
  bool faceOn = false,
}) {
  final c = math.cos(flip).abs();
  final squash = faceOn ? math.max(0.14, c) / 0.45 : 0.18 + 0.82 * c;
  return Positioned(
    left: 0,
    top: 0,
    child: Opacity(
      opacity: opacity.clamp(0.0, 1.0),
      child: Transform(
        transform: Matrix4.identity()
          ..translateByDouble(pos.dx, pos.dy, 0, 1)
          ..rotateZ(turn)
          ..scaleByDouble(size, size * squash, 1, 1)
          ..translateByDouble(-50, -28, 0, 1),
        child: coin,
      ),
    ),
  );
}

class SentArt extends StatefulWidget {
  const SentArt({super.key, required this.sent, this.sending = false});
  final bool sent;
  final bool sending;

  @override
  State<SentArt> createState() => _SentArtState();
}

class _SentArtState extends State<SentArt> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
  final ValueNotifier<double> _clock = ValueNotifier(0);

  double? _stoppedAt;

  static const _period = 2.6;
  static const _perTrail = 5;

  void _tick(Duration elapsed) {
    _clock.value = elapsed.inMicroseconds / 1e6;
    final stop = _stoppedAt;
    if (stop != null && _clock.value > stop + 0.7) _ticker.stop();
  }

  void _sync() {
    final still = MediaQuery.disableAnimationsOf(context);
    if (widget.sending && !widget.sent && !still) {
      _stoppedAt = null;
      if (!_ticker.isActive) {
        _clock.value = 0;
        _ticker.start();
      }
    } else if (_ticker.isActive) {
      _stoppedAt ??= _clock.value;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(SentArt old) {
    super.didUpdateWidget(old);
    _sync();
  }

  @override
  void dispose() {
    _ticker.dispose();
    _clock.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/sent_slot_${name}_${dark ? 'dark' : 'light'}.svg';
  }

  double _glow() {
    if (!_ticker.isActive) return widget.sent ? 0.35 : 1;
    final t = _clock.value;
    final breathe = 0.85 + 0.15 * math.sin(t * 2 * math.pi / 2.4);
    final stop = _stoppedAt;
    if (stop == null) return breathe;
    final close = Curves.easeOutCubic.transform(
      ((t - stop) / 0.5).clamp(0.0, 1.0),
    );
    return breathe + (0.35 - breathe) * close;
  }

  Widget _trailCoin(int side, int i, Widget coin) {
    final offset = (i + side * 0.5) / _perTrail * _period;
    final t = _clock.value - offset;
    final start = (t / _period).floorToDouble() * _period + offset;
    final stop = _stoppedAt;
    if (!_ticker.isActive || t < 0 || (stop != null && start > stop)) {
      return _placedCoin(coin, Offset.zero, 0, 0, 0, 0);
    }
    final phase = (t % _period) / _period;
    final r = _rand(side * 13 + i * 7 + start * 3.1);
    final u = phase * (1.02 + r * 0.08);
    final trail = _trails[side];
    final at = _on(trail, u);
    final dir = _on(trail, u + 0.01) - at;
    final normal = dir.distance == 0
        ? Offset.zero
        : Offset(-dir.dy, dir.dx) / dir.distance;
    final pos = at + normal * ((r - 0.5) * 30);
    final size = (0.95 - 0.5 * u) * (0.8 + r * 0.35);
    final flips = 2.0 + r * 2.5;
    final flip = phase * flips * math.pi + r * math.pi;
    final turn =
        ((r - 0.5) * 40 + phase * (side == 0 ? -30 : 30)) * math.pi / 180;
    final ended = stop == null
        ? 1.0
        : 1 - ((_clock.value - stop) / 0.6).clamp(0.0, 1.0);
    final opacity =
        math.min(1.0, phase * 14) *
        (1 - ((phase - 0.75) / 0.25).clamp(0.0, 1.0)) *
        ended;
    return _placedCoin(coin, pos, size, flip, turn, opacity * 0.92);
  }

  @override
  Widget build(BuildContext context) {
    Widget layer(String name) => RepaintBoundary(
      child: SvgPicture.asset(
        _layer(context, name),
        width: 1080,
        height: 2340,
        fit: BoxFit.fill,
      ),
    );
    final glow = layer('glow');
    final coin = SvgPicture.asset(
      _coinAsset(context),
      width: 100,
      height: 64,
      fit: BoxFit.fill,
    );
    return ExcludeSemantics(
      child: FittedBox(
        fit: BoxFit.cover,
        clipBehavior: Clip.hardEdge,
        child: SizedBox(
          width: 1080,
          height: 2340,
          child: AnimatedBuilder(
            animation: _clock,
            builder: (context, _) => Stack(
              children: [
                layer('bg'),
                Opacity(opacity: _glow(), child: glow),
                layer('fore'),
                for (var side = 0; side < 2; side++)
                  for (var i = 0; i < _perTrail; i++) _trailCoin(side, i, coin),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class CoinFountain extends StatefulWidget {
  const CoinFountain({super.key});

  static const size = Size(900, 1400);

  @override
  State<CoinFountain> createState() => _CoinFountainState();
}

class _CoinFountainState extends State<CoinFountain>
    with SingleTickerProviderStateMixin {
  static const _count = 14;
  static const _gravity = 2400.0;
  static const _life = 1.25;

  late final AnimationController _t = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!MediaQuery.disableAnimationsOf(context) &&
        !_t.isAnimating &&
        _t.value == 0) {
      _t.forward();
    }
  }

  @override
  void dispose() {
    _t.dispose();
    super.dispose();
  }

  Widget _coin(int i, Widget coin, Offset centre) {
    final r1 = _rand(i * 1.7 + 0.3),
        r2 = _rand(i * 3.1 + 1.1),
        r3 = _rand(i * 5.3 + 2.9);
    final t = _t.value * 1.4 - (0.08 + i * 0.011);
    if (t <= 0 || t > _life) {
      return _placedCoin(coin, Offset.zero, 0, 0, 0, 0);
    }
    final side = i.isEven ? 1 : -1;
    final angle = -math.pi / 2 + side * (0.35 + r1 * 0.85);
    final speed = 560 + r2 * 400;
    final v = Offset(math.cos(angle), math.sin(angle)) * speed;
    final pos = centre + v * t + Offset(0, _gravity * t * t / 2);
    final size = 0.24 + r3 * 0.18;
    final flip = t * (9 + r1 * 9) + r2 * math.pi;
    final turn = side * (r3 - 0.5) * 0.5 + t * side * 0.4;
    final opacity =
        math.min(1.0, t / 0.04) *
        (1 - ((t - (_life - 0.3)) / 0.3).clamp(0.0, 1.0));
    return _placedCoin(coin, pos, size, flip, turn, opacity, faceOn: true);
  }

  @override
  Widget build(BuildContext context) {
    final coin = SvgPicture.asset(
      _coinAsset(context),
      width: 100,
      height: 64,
      fit: BoxFit.fill,
    );
    final centre = Offset(
      CoinFountain.size.width / 2,
      CoinFountain.size.height / 2,
    );
    return IgnorePointer(
      child: ExcludeSemantics(
        child: SizedBox.fromSize(
          size: CoinFountain.size,
          child: AnimatedBuilder(
            animation: _t,
            builder: (context, _) => Stack(
              clipBehavior: Clip.none,
              children: [
                for (var i = 0; i < _count; i++) _coin(i, coin, centre),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

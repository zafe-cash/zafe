import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// The coin trails of the sent scene (start in the slot, control point, end) and where
/// each coin rests on its trail: `SENT_TRAILS` / `SENT_COIN_U` in
/// scripts/illustrations/scenes.py.
const _trails = [
  [Offset(480, 320), Offset(250, 560), Offset(70, 1000)],
  [Offset(600, 320), Offset(840, 520), Offset(1010, 940)],
];
const _coinU = [0.22, 0.48, 0.74];

Offset _on(List<Offset> t, double u) =>
    t[0] * ((1 - u) * (1 - u)) + t[1] * (2 * (1 - u) * u) + t[2] * (u * u);

/// The sending screen's background, animated from its layers (scenes.py `LAYERS`): while
/// the payment is [sending], a calm stream of coins leaves the slot along both dotted
/// trails; once [sent], the slot flashes gold and the whole stream speeds up, every coin
/// rushing out together, then the coins that stay settle in their resting places (1 s,
/// inside the time "Sent!" stays up). Drawn once per
/// layer and only moved; the sending screen's loader already draws every frame, so the
/// stream adds no frames. Reduced motion: no stream, the coins at rest once sent.
class SentArt extends StatefulWidget {
  const SentArt({super.key, required this.sent, this.sending = false});
  final bool sent;
  final bool sending;

  @override
  State<SentArt> createState() => _SentArtState();
}

class _SentArtState extends State<SentArt> with TickerProviderStateMixin {
  /// The stream's own clock. It runs at real speed while sending and speeds up when the
  /// payment is sent, so every coin in the air rushes out together and new ones follow
  /// closer: one motion, nothing overtakes anything (a separate burst passed the slow
  /// coins and looked wrong).
  late final Ticker _ticker = createTicker(_tick);
  final ValueNotifier<double> _clock = ValueNotifier(0);
  double _real = 0; // seconds since the ticker started
  double? _sentReal; // real time of "Sent!"
  double? _stoppedAt; // stream time after which no coin leaves

  static const _period = 3.0; // stream seconds a coin takes along its trail
  static const _perTrail = 7;
  static const _rush = 4.0; // the clock's speed once sent
  static const _rushRamp = 0.3; // real seconds to reach it
  static const _rushPour =
      1.6; // stream seconds coins keep leaving after "Sent!"

  void _tick(Duration elapsed) {
    final now = elapsed.inMicroseconds / 1e6;
    final dt = now - _real;
    _real = now;
    final sent = _sentReal;
    final rate = sent == null
        ? 1.0
        : 1 +
              (_rush - 1) *
                  Curves.easeIn.transform(
                    ((now - sent) / _rushRamp).clamp(0.0, 1.0),
                  );
    _clock.value += dt * rate;
    final stop = _stoppedAt;
    if (stop != null && _clock.value > stop + _period) {
      _ticker.stop();
      _clock.value = 0;
      _real = 0;
    }
  }

  void _syncStream() {
    final still = MediaQuery.disableAnimationsOf(context);
    if (widget.sending && !widget.sent && !still) {
      _stoppedAt = null;
      _sentReal = null;
      if (!_ticker.isActive) unawaited(_ticker.start());
    } else if (_ticker.isActive && _stoppedAt == null) {
      if (widget.sent) {
        _sentReal = _real;
        _stoppedAt = _clock.value + _rushPour;
      } else {
        _stoppedAt = _clock.value; // failed: the trickle just ends
      }
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncStream();
  }

  late final AnimationController _out = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1000),
    // Opened on a payment that was already sent: the coins are out.
    value: widget.sent ? 1 : 0,
  );

  @override
  void didUpdateWidget(SentArt old) {
    super.didUpdateWidget(old);
    _syncStream();
    if (widget.sent && !old.sent) {
      if (MediaQuery.disableAnimationsOf(context)) {
        _out.value = 1;
      } else {
        _out.forward();
      }
    } else if (!widget.sent && old.sent) {
      _out.value = 0; // a retry after a failure starts over
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    _clock.dispose();
    _out.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/sent_slot_${name}_${dark ? 'dark' : 'light'}.svg';
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
    final stream = SvgPicture.asset(
      _layer(context, 'coin'),
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
          child: Stack(
            children: [
              layer('bg'),
              AnimatedBuilder(
                animation: _clock,
                builder: (context, _) => Stack(
                  children: [
                    _flash(context),
                    for (var side = 0; side < 2; side++)
                      for (var i = 0; i < _perTrail; i++)
                        _streamCoin(side, i, stream),
                  ],
                ),
              ),
              AnimatedBuilder(
                animation: _out,
                builder: (context, _) => Stack(
                  children: [
                    for (var side = 0; side < 2; side++)
                      for (var k = 0; k < 3; k++) _coin(side, k, stream),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// One coin of the stream: coin `i` of a trail leaves the slot every [_period]
  /// seconds, offset from its neighbours, travels a little past the trail's end while
  /// shrinking and turning, and fades in and out. Coins that would leave after the
  /// stream stopped stay inside.
  Widget _streamCoin(int side, int i, Widget coin) {
    // Hidden coins keep their place in the tree (opacity 0 paints nothing): swapping
    // them for an empty box made each one reload its picture and come back blank.
    final hidden = _streamPlaced(Matrix4.identity(), 0, coin);
    if (!_ticker.isActive) return hidden;
    final offset = (i + side * 0.5) / _perTrail * _period;
    final t = _clock.value - offset;
    if (t < 0) return hidden;
    final start = (t / _period).floorToDouble() * _period + offset;
    final stop = _stoppedAt;
    if (stop != null && start > stop) return hidden;
    return _trip(
      side,
      (t % _period) / _period,
      side * 13 + i * 7 + start * 3.1,
      Curves.linear,
      1,
      coin,
    );
  }

  /// A gold flash from the slot as the payment goes out.
  Widget _flash(BuildContext context) {
    final sent = _sentReal;
    final t = sent == null ? 1.0 : (_real - sent) / 0.7;
    final glow = t < 0 || t >= 1 ? 0.0 : math.sin(t * math.pi) * 0.55;
    final gold = context.colors.icon.value;
    return Positioned(
      left: 540 - 300,
      top: 300 - 220,
      child: Opacity(
        opacity: glow,
        child: Container(
          width: 600,
          height: 440,
          decoration: BoxDecoration(
            gradient: RadialGradient(
              colors: [gold.withValues(alpha: 0.9), gold.withValues(alpha: 0)],
            ),
          ),
        ),
      ),
    );
  }

  /// A coin `phase` (0-1) of the way along its trail: a little past the end, shrinking
  /// and turning, fading in and out, nudged sideways by a fixed wobble from `seed`.
  Widget _trip(
    int side,
    double phase,
    double seed,
    Curve curve,
    double size,
    Widget coin,
  ) {
    final w = math.sin(seed * 12.9898) * 43758.5453;
    final r = w - w.floorToDouble();
    final u = curve.transform(phase) * (1.02 + r * 0.08);
    final trail = _trails[side];
    final at = _on(trail, u);
    final dir = _on(trail, u + 0.01) - at;
    final normal = dir.distance == 0
        ? Offset.zero
        : Offset(-dir.dy, dir.dx) / dir.distance;
    final pos = at + normal * ((r - 0.5) * 36);
    final scale = (0.95 - 0.55 * u) * (0.8 + r * 0.35) * size;
    final turn =
        ((r - 0.5) * 50 + phase * (side == 0 ? -40 : 40)) * math.pi / 180;
    final opacity =
        math.min(1.0, phase * 8) * (1 - ((phase - 0.7) / 0.3).clamp(0.0, 1.0));
    return _streamPlaced(
      Matrix4.identity()
        ..translateByDouble(pos.dx, pos.dy, 0, 1)
        ..rotateZ(turn)
        ..scaleByDouble(scale, scale, 1, 1)
        ..translateByDouble(-50, -28, 0, 1),
      opacity * 0.9,
      coin,
    );
  }

  Widget _streamPlaced(Matrix4 transform, double opacity, Widget coin) =>
      Positioned(
        left: 0,
        top: 0,
        child: Opacity(
          opacity: opacity,
          child: Transform(transform: transform, child: coin),
        ),
      );

  /// The coins that stay, settling into their places in the still scene (size, tilt and
  /// fade as drawn there) once the rush has passed them, nearest first: a short drop
  /// along the trail and a small bounce, so they read as coins coming to rest.
  Widget _coin(int side, int k, Widget coin) {
    final u = _coinU[k];
    final start = 0.42 + k * 0.13 + side * 0.04;
    final local = ((_out.value - start) / 0.34).clamp(0.0, 1.0);
    // Opacity 0 until it lands, never removed: its picture stays loaded.
    if (local == 0) return _streamPlaced(Matrix4.identity(), 0, coin);
    final e = Curves.easeOutBack.transform(local);
    final pos = _on(_trails[side], u - 0.06 * (1 - e));
    // The scene's coin: radius 40 - 9k (the asset's is 40), tilted -24 + 14k degrees
    // (mirrored on the right), fading 22% per step along the trail.
    final rest = (40 - 9 * k) / 40;
    final scale = rest * (0.7 + 0.3 * e);
    final tilt = (-24 + 14 * k) * (side == 0 ? -1 : 1) * math.pi / 180;
    final opacity = math.min(1.0, local * 3) * (1 - 0.22 * k);
    return _streamPlaced(
      Matrix4.identity()
        ..translateByDouble(pos.dx, pos.dy, 0, 1)
        ..rotateZ(tilt)
        ..scaleByDouble(scale, scale, 1, 1)
        ..translateByDouble(-50, -28, 0, 1),
      opacity,
      coin,
    );
  }
}

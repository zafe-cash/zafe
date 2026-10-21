import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// Where each shard sits apart from its place in the whole key (SVG units and degrees):
/// the same numbers as `SHARD_OFFSETS` in scripts/illustrations/scenes.py.
const _offsets = [
  (-80.0, -18.0, -7.0),
  (-30.0, 48.0, 6.0),
  (0.0, -46.0, -4.0),
  (34.0, 42.0, 6.0),
  (84.0, -14.0, 10.0),
];

/// How far apart the shards stay once joined (a fraction of their resting offsets).
const _gap = 0.14;

/// The key's centre in the 1080 x 1240 scene, which every shard turns around.
const _cx = 540.0, _cy = 470.0;

/// The key-shards scene, animated from its layers (scripts/illustrations: `LAYERS`):
/// while [assembled] is false the five shards drift gently apart; when it turns true
/// they close into the dashed outline of the whole key (900 ms) and settle with a small
/// pulse, then [onAssembled] runs. They stop short of touching: the shares act together,
/// but the whole key never exists, and the art shouldn't say otherwise. Each layer is drawn once and only moved, and
/// nothing runs while the screen is hidden; reduced motion shows it still and joins it
/// at once.
class KeyShardsArt extends StatefulWidget {
  const KeyShardsArt({super.key, this.assembled = false, this.onAssembled});
  final bool assembled;
  final VoidCallback? onAssembled;

  @override
  State<KeyShardsArt> createState() => _KeyShardsArtState();
}

class _KeyShardsArtState extends State<KeyShardsArt>
    with TickerProviderStateMixin {
  late final AnimationController _drift = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 6),
  );
  late final AnimationController _join = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
    value: widget.assembled ? 1 : 0,
  );
  late final AnimationController _pulse = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 420),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final still = MediaQuery.disableAnimationsOf(context);
    if (still) {
      _drift.stop();
    } else if (!_drift.isAnimating && _join.value < 1) {
      _drift.repeat();
    }
  }

  @override
  void didUpdateWidget(KeyShardsArt old) {
    super.didUpdateWidget(old);
    if (widget.assembled && !old.assembled) unawaited(_assemble());
  }

  Future<void> _assemble() async {
    if (MediaQuery.disableAnimationsOf(context)) {
      _join.value = 1;
      await Future<void>.delayed(const Duration(milliseconds: 400));
    } else {
      await _join.animateTo(1, curve: Curves.easeInOutCubic);
      _drift.stop();
      await _pulse.forward(from: 0);
      await Future<void>.delayed(const Duration(milliseconds: 350));
    }
    if (mounted) widget.onAssembled?.call();
  }

  @override
  void dispose() {
    _drift.dispose();
    _join.dispose();
    _pulse.dispose();
    super.dispose();
  }

  String _layer(BuildContext context, String name) {
    final dark = identical(context.colors, AppColors.dark);
    return 'assets/illustrations/layers/key_shards_${name}_${dark ? 'dark' : 'light'}.svg';
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
    final pieces = [for (var i = 0; i < 5; i++) layer('piece$i')];
    return FittedBox(
      fit: BoxFit.cover,
      child: SizedBox(
        width: 1080,
        height: 1240,
        child: Stack(
          children: [
            layer('bg'),
            AnimatedBuilder(
              animation: Listenable.merge([_drift, _join, _pulse]),
              builder: (context, _) {
                final apart = 1 - _join.value * (1 - _gap);
                // A quick swell and settle once the key is whole.
                final swell = math.sin(_pulse.value * math.pi) * 0.035;
                return Transform.scale(
                  scale: 1 + swell,
                  origin: const Offset(_cx - 540, _cy - 620),
                  child: Stack(
                    children: [
                      for (var i = 0; i < 5; i++)
                        Transform(
                          transform: _pieceTransform(i, apart),
                          child: pieces[i],
                        ),
                    ],
                  ),
                );
              },
            ),
            layer('top'),
          ],
        ),
      ),
    );
  }

  /// Moves a shard drawn in its place out by [apart] (1: as in the still scene), plus
  /// its drift: the transform of the scene's group, relative to the key's place.
  Matrix4 _pieceTransform(int i, double apart) {
    final (dx, dy, dr) = _offsets[i];
    final phase = (_drift.value + i * 0.21) * 2 * math.pi;
    final float = 1 - _join.value; // drift stops as the shards join
    final ox = dx * apart;
    final oy = dy * apart + math.sin(phase) * 6 * float;
    final turn =
        (dr * apart + math.sin(phase + 1.3) * 1.2 * float) * math.pi / 180;
    return Matrix4.identity()
      ..translateByDouble(_cx + ox, _cy + oy, 0, 1)
      ..rotateZ(turn)
      ..translateByDouble(-_cx, -_cy, 0, 1);
  }
}

import 'dart:math' as math;

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
/// the payment is on its way the coins stay in the vault; once [sent], they fly out of
/// the slot along their dotted trails, nearest first (1 s, inside the time "Sent!" stays
/// up). Drawn once per layer and only moved; reduced motion shows them at rest.
class SentArt extends StatefulWidget {
  const SentArt({super.key, required this.sent});
  final bool sent;

  @override
  State<SentArt> createState() => _SentArtState();
}

class _SentArtState extends State<SentArt> with SingleTickerProviderStateMixin {
  late final AnimationController _out = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1000),
    // Opened on a payment that was already sent: the coins are out.
    value: widget.sent ? 1 : 0,
  );

  @override
  void didUpdateWidget(SentArt old) {
    super.didUpdateWidget(old);
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
    final coins = [
      for (var side = 0; side < 2; side++)
        for (var k = 0; k < 3; k++) (side, k, layer('coin$side$k')),
    ];
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
                animation: _out,
                builder: (context, _) => Stack(
                  children: [
                    for (final (side, k, child) in coins) _coin(side, k, child),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// One coin, drawn in its resting place, moved back along its trail by how far it
  /// still has to go. The nearest coins leave first; the far ones follow.
  Widget _coin(int side, int k, Widget child) {
    final start = k * 0.16 + side * 0.06;
    final local = ((_out.value - start) / 0.62).clamp(0.0, 1.0);
    if (local == 0) return const SizedBox.shrink();
    final e = Curves.easeOutCubic.transform(local);
    final trail = _trails[side];
    final rest = _on(trail, _coinU[k]);
    final now = _on(trail, _coinU[k] * e);
    final scale = 0.35 + 0.65 * e;
    return Opacity(
      opacity: math.min(1, local * 4),
      child: Transform(
        transform: Matrix4.identity()
          ..translateByDouble(now.dx, now.dy, 0, 1)
          ..scaleByDouble(scale, scale, 1, 1)
          ..translateByDouble(-rest.dx, -rest.dy, 0, 1),
        child: child,
      ),
    );
  }
}

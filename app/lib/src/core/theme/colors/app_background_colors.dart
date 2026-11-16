import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppBackgroundColors {
  const AppBackgroundColors({
    required this.window,
    required this.ground,
    required this.base,
    required this.raised,
    required this.overlay,
    required this.inverse,
    required this.neutralScrim,
    required this.neutralSubtleOpacity,
    required this.neutralStrongOpacity,
    required this.brandSubtle,
    required this.brandStrong,
    required this.brandAlpha,
    required this.destructiveSubtle,
    required this.destructiveStrong,
    required this.destructiveAlphaSubtle,
    required this.destructiveAlpha,
    required this.valueAlpha,
    required this.darkCard,
  });

  final Color window;
  final Color ground;
  final Color base;
  final Color raised;
  final Color overlay;
  final Color inverse;
  final Color neutralScrim;
  final Color neutralSubtleOpacity;
  final Color neutralStrongOpacity;
  final Color brandSubtle;
  final Color brandStrong;
  final Color brandAlpha;
  final Color destructiveSubtle;
  final Color destructiveStrong;
  final Color destructiveAlphaSubtle;
  final Color destructiveAlpha;
  final Color valueAlpha;
  final Color darkCard;

  static const dark = AppBackgroundColors(
    window: Color(0xFF080B0B),
    ground: Primitives.p50Dark,
    base: Primitives.p100Dark,
    raised: Primitives.p150Dark,
    overlay: Primitives.p200Dark,
    inverse: Primitives.p800Dark,
    neutralScrim: Primitives.p0Alpha50Dark,
    neutralSubtleOpacity: Primitives.p400Alpha20Dark,
    neutralStrongOpacity: Primitives.p300Alpha50Dark,
    brandSubtle: BrandPrimitives.p100Dark,
    brandStrong: BrandPrimitives.p400Dark,
    brandAlpha: BrandPrimitives.p300Alpha35Dark,
    destructiveSubtle: RosePrimitives.p50Dark,
    destructiveStrong: RosePrimitives.p300Dark,
    destructiveAlphaSubtle: RosePrimitives.p400Alpha8Dark,
    destructiveAlpha: RosePrimitives.p400Alpha25Dark,
    valueAlpha: GoldPrimitives.p500Alpha15Dark,
    darkCard: Primitives.p50Dark,
  );

  static const light = AppBackgroundColors(
    window: Primitives.p50Light,
    ground: Primitives.p0Light,
    base: Primitives.p50Light,
    raised: Primitives.p100Light,
    overlay: Primitives.p150Light,
    inverse: Primitives.p800Light,
    neutralScrim: Primitives.p900Alpha50Light,
    neutralSubtleOpacity: Primitives.p300Alpha20Light,
    neutralStrongOpacity: Primitives.p300Alpha35Light,
    brandSubtle: BrandPrimitives.p0Light,
    brandStrong: BrandPrimitives.p300Light,
    brandAlpha: BrandPrimitives.p300Alpha15Light,
    destructiveSubtle: RosePrimitives.p0Light,
    destructiveStrong: RosePrimitives.p400Light,
    destructiveAlphaSubtle: RosePrimitives.p400Alpha8Light,
    destructiveAlpha: RosePrimitives.p400Alpha15Light,
    valueAlpha: GoldPrimitives.p400Alpha15Light,
    darkCard: Primitives.p800Light,
  );
}

import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppNavPanelColors {
  const AppNavPanelColors({
    required this.badgeBg,
    required this.activeBg,
    required this.activeIcon,
    required this.activeLabel,
  });

  final Color badgeBg;
  final Color activeBg;
  final Color activeIcon;
  final Color activeLabel;

  static const dark = AppNavPanelColors(
    badgeBg: BrandPrimitives.p300Dark,
    activeBg: BrandPrimitives.p400Alpha15Dark,
    activeIcon: BrandPrimitives.p400Dark,
    activeLabel: BrandPrimitives.p900Dark,
  );

  static const light = AppNavPanelColors(
    badgeBg: BrandPrimitives.p300Light,
    activeBg: BrandPrimitives.p300Alpha10Light,
    activeIcon: BrandPrimitives.p300Light,
    activeLabel: BrandPrimitives.p800Light,
  );
}

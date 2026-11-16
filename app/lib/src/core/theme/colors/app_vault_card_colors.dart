import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppVaultCardColors {
  const AppVaultCardColors({
    required this.background,
    required this.text,
    required this.textSecondary,
    required this.accent,
    required this.emptyDot,
    required this.ticker,
    required this.chip,
    required this.inset,
    required this.insetBorder,
  });

  final Color background;
  final Color text;
  final Color textSecondary;
  final Color accent;
  final Color emptyDot;
  final Color ticker;
  final Color chip;
  final Color inset;
  final Color insetBorder;

  static const dark = AppVaultCardColors(
    background: Primitives.p50Dark,
    text: Primitives.p800Dark,
    textSecondary: Color(0xB3F1F5F5),
    accent: BrandPrimitives.p400Dark,
    emptyDot: Color(0x66F1F5F5),
    ticker: GoldPrimitives.p500Dark,
    chip: Color(0x14FFFFFF),
    inset: Color(0x0DFFFFFF),
    insetBorder: Color(0x14FFFFFF),
  );

  static const light = AppVaultCardColors(
    background: Primitives.p0Light,
    text: Primitives.p900Light,
    textSecondary: Primitives.p600Light,
    accent: BrandPrimitives.p300Light,
    emptyDot: Primitives.p500Light,
    ticker: GoldPrimitives.p400Light,
    chip: Color(0x0F090E0E),
    inset: Primitives.p100Light,
    insetBorder: Primitives.p150Light,
  );
}

import 'package:flutter/painting.dart';

import '../primitives.dart';

/// The vault card (Home's balance card) and the payment card that mirrors it
/// (docs/brand.md §3, "Vault card in light mode").
///
/// Plain since 2026-10-27 (user's pick after comparing the dial, a glow and tinted
/// cards): the surface every other card uses, nothing drawn on it, the Zcash mark and
/// "Vault balance" in the secondary text colour.
///
/// * [background] — The card itself (the cards' ground surface).
/// * [text] / [textSecondary] — Amount, and the label, mark, notes and threshold text.
/// * [accent] — Filled signer dots and the recipient's shield.
/// * [emptyDot] — Outline of a signer dot beyond the threshold.
/// * [ticker] — The currency ticker next to amounts: Zcash gold, the value colour.
/// * [chip] — Small buttons on the card (show/hide balance, full address).
/// * [inset] / [insetBorder] — The recipient box on the payment card.
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

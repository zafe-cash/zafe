/// Typography tokens from the Figma design system.
///
/// The Figma `Fonts` variable collection has a Desktop and a Mobile mode
/// (`3 Fonts-3.zip`). Both modes are materialized as complete const
/// token sets — [AppTypographyDesktop] and [AppTypographyMobile] — and
/// [AppTypography] selects between them at **compile time** via
/// [kAppFormFactor], so call sites keep the familiar
/// `AppTypography.bodyMedium` shape, stay const, and the unused set is
/// tree-shaken out of release builds. Reference a mode set directly only
/// in tooling (widgetbook galleries, token tests) that must show both
/// modes inside one binary.
///
/// Mobile scale (2026-10-27, after reading research and a side-by-side comparison; see
/// docs/brand.md "Type"): six sizes. Space Grotesk for amounts and headings: [hero] 44,
/// [title] 28, [heading] 22, [amount] 18; DM Sans for text: 18 (headlineSmall, bodyLarge),
/// 16 (body, labels), 14 (small text and captions, the minimum: below it reading slows,
/// Legge & Bigelow 2011); JetBrains Mono 14 for addresses and codes. Every amount uses
/// tabular figures ([kTabularFigures]) so digits don't shift as values change; DM Sans has
/// none, so amounts never use it. Space Grotesk's digits are lining by default (the
/// bundled files have no `lnum` or `case` feature).
///
/// Naming maps Figma → Dart by full word and camelCase where possible:
/// `Headline L` → `headlineLarge`, `Body L` → `bodyLarge`,
/// `Label S` → `labelSmall`, `Code M` → `codeMedium`. The one outlier is
/// `Body M Medium`, the emphasis variant of the regular body — surfaced
/// as [AppTypography.bodyMediumStrong]. The old `display*` constants are
/// legacy aliases onto the headline scale (`displayLarge` /
/// `displayMedium` → `Headline XL`, `displaySmall` → `Headline L`).
///
/// Font sizes and letter spacings are authored in logical pixels. The
/// sans families and letter spacings are identical across modes; the
/// serif display scale differs by size between modes. Line heights are
/// stored as the unitless multiplier Flutter expects (`TextStyle.height`)
/// — computed as `figmaLineHeightPx / fontSizePx` so the original Figma
/// design still reproduces exactly.
///
/// Colors are not baked into these styles. Callers merge colors in at
/// the call site (usually through `DefaultTextStyle.merge` or
/// `style.copyWith(color: context.colors.text.primary)`). This keeps
/// the token a pure typographic concern and lets it work with whichever
/// semantic text color the caller needs.
library;

import 'package:flutter/widgets.dart';

import '../layout/app_form_factor.dart';

// ─── Mode-invariant styles (identical in both Figma modes) ───────────

/// Equal-width digits for amounts: a value that changes (a balance, the send field, a
/// countdown) or sits in a column keeps its width. Space Grotesk has `tnum`; DM Sans
/// doesn't.
const kTabularFigures = [FontFeature.tabularFigures()];

// ─── Roles shared by both form factors ───────────────────────────────

/// The balance and a payment's amount: Space Grotesk 44 / 48, tabular.
const _hero = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 44,
  height: 48 / 44,
  letterSpacing: -0.88,
  fontFeatures: kTabularFigures,
);

/// The ticker next to a [_hero] amount: half its size.
const _heroTicker = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 22,
  height: 28 / 22,
  letterSpacing: 0,
);

/// Large headings in a screen's body ("Zafe is locked", "Grants is ready").
const _title = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 28,
  height: 34 / 28,
  letterSpacing: -0.28,
  fontFeatures: kTabularFigures,
);

/// Screen titles in the top bar and step headings.
const _heading = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 22,
  height: 28 / 22,
  letterSpacing: 0,
);

/// Amounts in rows and cards: Space Grotesk 18 / 24, tabular.
const _amount = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 18,
  height: 24 / 18,
  letterSpacing: 0,
  fontFeatures: kTabularFigures,
);

/// The ticker next to an [_amount].
const _amountTicker = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 14,
  height: 24 / 14,
  letterSpacing: 0,
);

/// Addresses, codes and the diagnostic report: JetBrains Mono 14 / 20 (dotted zero,
/// distinct Il1).
const _mono = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontWeight: FontWeight.w400,
  fontSize: 14,
  height: 20 / 14,
  letterSpacing: 0,
);

/// Desktop `Headline L` — Space Grotesk, 32 / 33 px.
const _headlineLDesktop = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 32,
  height: 33 / 32,
  letterSpacing: 0,
  fontFeatures: kTabularFigures,
);

/// Figma desktop `Headline M` — Space Grotesk, 28 / 30 px, −0.28.
const _headlineMDesktop = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 28,
  height: 30 / 28,
  letterSpacing: -0.28,
  fontFeatures: kTabularFigures,
);

/// Figma `Code M` — JetBrains Mono Medium, 14 / 21 px.
const _codeMDesktop = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontWeight: FontWeight.w500,
  fontSize: 14,
  height: 21 / 14,
  letterSpacing: 0,
);

/// Desktop `Code S` — JetBrains Mono Medium, 13 / 17 px (the mobile scale uses [_mono]).
const _codeS = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontWeight: FontWeight.w500,
  // type: desktop only; the mobile minimum is 14.
  fontSize: 13,
  height: 17 / 13,
  letterSpacing: 0,
);

/// The Desktop mode of the Figma `Fonts` collection.
abstract final class AppTypographyDesktop {
  /// Figma `Headline XL` — 45 / 48 px, letter-spacing −1.35.
  static const displayLarge = TextStyle(
    fontFamily: 'Space Grotesk',
    fontWeight: FontWeight.w500,
    fontFeatures: kTabularFigures,
    fontSize: 45,
    height: 48 / 45,
    letterSpacing: -1.35,
  );

  static const displayMedium = displayLarge;
  static const displaySmall = _headlineLDesktop;
  static const headlineLarge = _headlineLDesktop;
  static const headlineMedium = _headlineMDesktop;

  /// Figma `Headline S` — DM Sans Medium, 16 / 20 px.
  static const headlineSmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 16,
    height: 20 / 16,
    letterSpacing: 0,
  );

  /// Figma `Body L` — DM Sans Medium, 16 / 24 px, −0.24.
  static const bodyLarge = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 16,
    height: 24 / 16,
    letterSpacing: -0.24,
  );

  /// Figma `Body M` — DM Sans Regular, 14 / 21 px, −0.21.
  static const bodyMedium = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 14,
    height: 21 / 14,
    letterSpacing: -0.21,
  );

  /// Figma `Body M Medium` — DM Sans Medium, 14 / 21 px, −0.21.
  static const bodyMediumStrong = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 14,
    height: 21 / 14,
    letterSpacing: -0.21,
  );

  /// Figma `Body S` — DM Sans Regular, 12 / 18 px, −0.12.
  static const bodySmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 12,
    height: 18 / 12,
    letterSpacing: -0.12,
  );

  /// Figma `Body XS` — DM Sans Regular, 11 / 16 px, −0.055.
  static const bodyExtraSmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 11,
    height: 16 / 11,
    letterSpacing: -0.055,
  );

  /// Figma `Label M` — DM Sans Medium, 14 / 16 px, −0.06.
  static const labelLarge = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 14,
    height: 16 / 14,
    letterSpacing: -0.06,
  );

  /// Figma `Label S` — DM Sans Medium, 13 / 14 px.
  static const labelMedium = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 13,
    height: 14 / 13,
    letterSpacing: 0,
  );

  static const labelSmall = labelMedium;
  static const codeMedium = _codeMDesktop;
  static const codeSmall = _codeS;
}

/// The mobile scale: six sizes (44, 28, 22, 18, 16, 14), see the library doc.
abstract final class AppTypographyMobile {
  static const displayLarge = _hero;
  static const displayMedium = _hero;
  static const displaySmall = _title;
  static const headlineLarge = _title;
  static const headlineMedium = _title;

  /// Subheadings: DM Sans Medium 18 / 24.
  static const headlineSmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 18,
    height: 24 / 18,
    letterSpacing: -0.18,
  );
  static const bodyLarge = headlineSmall;

  /// Body text: DM Sans 16 / 24.
  static const bodyMedium = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 16,
    height: 24 / 16,
    letterSpacing: 0,
  );

  static const bodyMediumStrong = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 16,
    height: 24 / 16,
    letterSpacing: 0,
  );

  /// Small text and captions: DM Sans 14 / 20, the smallest size.
  static const bodySmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 14,
    height: 20 / 14,
    letterSpacing: 0,
  );

  /// Was 13; 14 is the minimum now (13 sat below the size where reading slows).
  static const bodyExtraSmall = bodySmall;

  /// Buttons, rows and nav: DM Sans Medium 16 / 20 (room for a wrapped second line).
  static const labelLarge = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 16,
    height: 20 / 16,
    letterSpacing: 0,
  );

  /// Small labels: DM Sans Medium 14 / 20.
  static const labelMedium = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 14,
    height: 20 / 14,
    letterSpacing: 0,
  );

  static const labelSmall = labelMedium;
  static const codeMedium = _mono;
  static const codeSmall = _mono;
}

const _mobile = kAppFormFactor == AppFormFactor.mobile;

/// The typography tokens for the form factor this binary was built for.
///
/// See the library doc above for the selection mechanism and
/// [AppTypographyDesktop] / [AppTypographyMobile] for per-style Figma
/// metrics.
abstract final class AppTypography {
  // ─── Roles (both form factors) ──────────────────────────────────

  /// The balance and a payment's amount (Space Grotesk 44, tabular).
  static const hero = _hero;

  /// The ticker next to a [hero] amount (22).
  static const heroTicker = _heroTicker;

  /// Large headings in a screen's body (Space Grotesk 28).
  static const title = _title;

  /// Screen titles and step headings (Space Grotesk 22).
  static const heading = _heading;

  /// Amounts in rows and cards (Space Grotesk 18, tabular).
  static const amount = _amount;

  /// The ticker next to an [amount] (14).
  static const amountTicker = _amountTicker;

  /// Addresses and codes (JetBrains Mono 14).
  static const mono = _mono;

  // ─── Display (legacy aliases onto the headline scale) ─────────────

  /// Largest onboarding/welcome headline (Figma `Headline XL`).
  static const displayLarge = _mobile
      ? AppTypographyMobile.displayLarge
      : AppTypographyDesktop.displayLarge;

  /// Hero headlines (Figma `Headline XL`).
  static const displayMedium = displayLarge;

  /// Step-level headlines inside onboarding flows (Figma `Headline L`).
  static const displaySmall = _mobile
      ? AppTypographyMobile.displaySmall
      : AppTypographyDesktop.displaySmall;

  // ─── Headline ─────────────────────────────────────────────────────

  /// Section headings inside content panes (Figma `Headline L`).
  static const headlineLarge = _mobile
      ? AppTypographyMobile.headlineLarge
      : AppTypographyDesktop.headlineLarge;

  /// Sub-section headings (Figma `Headline M`).
  static const headlineMedium = _mobile
      ? AppTypographyMobile.headlineMedium
      : AppTypographyDesktop.headlineMedium;

  /// Card titles, group labels (Figma `Headline S`).
  static const headlineSmall = _mobile
      ? AppTypographyMobile.headlineSmall
      : AppTypographyDesktop.headlineSmall;

  // ─── Body ─────────────────────────────────────────────────────────

  /// Comfortable paragraph copy, intro descriptions (Figma `Body L`).
  static const bodyLarge = _mobile
      ? AppTypographyMobile.bodyLarge
      : AppTypographyDesktop.bodyLarge;

  /// Default paragraph and subtitle copy (Figma `Body M`).
  static const bodyMedium = _mobile
      ? AppTypographyMobile.bodyMedium
      : AppTypographyDesktop.bodyMedium;

  /// Emphasis variant of [bodyMedium]; same metrics, medium weight. Use
  /// for inline emphasis where italic / bold would over-shout (Figma
  /// `Body M Medium`).
  static const bodyMediumStrong = _mobile
      ? AppTypographyMobile.bodyMediumStrong
      : AppTypographyDesktop.bodyMediumStrong;

  /// Fine print, legal footers, metadata (Figma `Body S`).
  static const bodySmall = _mobile
      ? AppTypographyMobile.bodySmall
      : AppTypographyDesktop.bodySmall;

  /// Smallest readable copy: footnotes, dense table cells, chip text
  /// (Figma `Body XS`).
  static const bodyExtraSmall = _mobile
      ? AppTypographyMobile.bodyExtraSmall
      : AppTypographyDesktop.bodyExtraSmall;

  // ─── Label ────────────────────────────────────────────────────────

  /// Button labels and nav item text (Figma `Label M`).
  static const labelLarge = _mobile
      ? AppTypographyMobile.labelLarge
      : AppTypographyDesktop.labelLarge;

  /// Compact label copy (Figma `Label S`).
  static const labelMedium = _mobile
      ? AppTypographyMobile.labelMedium
      : AppTypographyDesktop.labelMedium;

  /// Micro-copy: tag pills, status badges, dense controls (Figma
  /// `Label S`).
  static const labelSmall = labelMedium;

  // ─── Code ─────────────────────────────────────────────────────────
  // JetBrains Mono — see `pubspec.yaml`. Use for content where character
  // alignment matters: addresses, transaction IDs, mnemonics, hex
  // dumps.

  /// Primary monospace copy (Figma `Code M`).
  static const codeMedium = _mobile
      ? AppTypographyMobile.codeMedium
      : AppTypographyDesktop.codeMedium;

  /// Secondary monospace copy: mnemonic word indices, compact numeric
  /// metadata (Figma `Code S`).
  static const codeSmall = _mobile
      ? AppTypographyMobile.codeSmall
      : AppTypographyDesktop.codeSmall;
}

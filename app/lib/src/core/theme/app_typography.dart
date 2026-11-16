library;

import 'package:flutter/widgets.dart';

const kTabularFigures = [FontFeature.tabularFigures()];

const _hero = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 44,
  height: 48 / 44,
  letterSpacing: -0.88,
  fontFeatures: kTabularFigures,
);

const _heroTicker = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 22,
  height: 28 / 22,
  letterSpacing: 0,
);

const _title = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 28,
  height: 34 / 28,
  letterSpacing: -0.28,
  fontFeatures: kTabularFigures,
);

const _heading = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 22,
  height: 28 / 22,
  letterSpacing: 0,
);

const _amount = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 18,
  height: 24 / 18,
  letterSpacing: 0,
  fontFeatures: kTabularFigures,
);

const _amountTicker = TextStyle(
  fontFamily: 'Space Grotesk',
  fontWeight: FontWeight.w500,
  fontSize: 14,
  height: 24 / 14,
  letterSpacing: 0,
);

const _mono = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontWeight: FontWeight.w400,
  fontSize: 14,
  height: 20 / 14,
  letterSpacing: 0,
);

abstract final class AppTypography {
  static const hero = _hero;

  static const heroTicker = _heroTicker;

  static const title = _title;

  static const heading = _heading;

  static const amount = _amount;

  static const amountTicker = _amountTicker;

  static const mono = _mono;

  static const displayLarge = _hero;

  static const displayMedium = _hero;

  static const displaySmall = _title;

  static const headlineLarge = _title;

  static const headlineMedium = _title;

  static const headlineSmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 18,
    height: 24 / 18,
    letterSpacing: -0.18,
  );

  static const bodyLarge = headlineSmall;

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

  static const bodySmall = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w400,
    fontSize: 14,
    height: 20 / 14,
    letterSpacing: 0,
  );

  static const bodyExtraSmall = bodySmall;

  static const labelLarge = TextStyle(
    fontFamily: 'DM Sans',
    fontWeight: FontWeight.w500,
    fontSize: 16,
    height: 20 / 16,
    letterSpacing: 0,
  );

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

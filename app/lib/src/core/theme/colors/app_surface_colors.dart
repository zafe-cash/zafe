import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppSurfaceColors {
  const AppSurfaceColors({
    required this.card,
    required this.input,
    required this.nav,
    required this.navActive,
    required this.tooltip,
    required this.qrCode,
    required this.scrollbarThumb,
  });

  final Color card;
  final AppInputSurfaceColors input;
  final Color nav;
  final Color navActive;
  final Color tooltip;
  final Color qrCode;
  final Color scrollbarThumb;

  static const dark = AppSurfaceColors(
    card: Primitives.p100Dark,
    input: AppInputSurfaceColors.dark,
    nav: Primitives.p50Dark,
    navActive: Primitives.p150Dark,
    tooltip: Primitives.p200Dark,
    qrCode: Primitives.p0Light,
    scrollbarThumb: Primitives.p200Dark,
  );

  static const light = AppSurfaceColors(
    card: Primitives.p50Light,
    input: AppInputSurfaceColors.light,
    nav: Primitives.p0Light,
    navActive: Primitives.p100Light,
    tooltip: Primitives.p800Light,
    qrCode: Primitives.p0Light,
    scrollbarThumb: Primitives.p150Light,
  );
}

class AppInputSurfaceColors {
  const AppInputSurfaceColors({
    required this.primary,
    required this.secondary,
    required this.focus,
  });

  final Color primary;
  final Color secondary;
  final Color focus;

  static const dark = AppInputSurfaceColors(
    primary: Primitives.p50Dark,
    secondary: Primitives.p150Dark,
    focus: Primitives.p100Dark,
  );

  static const light = AppInputSurfaceColors(
    primary: Primitives.p0Light,
    secondary: Primitives.p100Light,
    focus: Primitives.p50Light,
  );
}

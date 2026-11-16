import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppIconColors {
  const AppIconColors({
    required this.accent,
    required this.regular,
    required this.muted,
    required this.disabled,
    required this.inverse,
    required this.onPrimary,
    required this.warning,
    required this.destructive,
    required this.destructiveLight,
    required this.value,
    required this.brand,
  });

  final Color accent;
  final Color regular;
  final Color muted;
  final Color disabled;
  final Color inverse;
  final Color onPrimary;
  final Color warning;
  final Color destructive;
  final Color destructiveLight;
  final Color value;
  final Color brand;

  static const dark = AppIconColors(
    accent: Primitives.p800Dark,
    regular: Primitives.p700Dark,
    muted: Primitives.p500Dark,
    disabled: Primitives.p300Dark,
    inverse: Primitives.p0Dark,
    onPrimary: Primitives.p0Dark,
    warning: OrangePrimitives.p500Dark,
    destructive: RosePrimitives.p400Dark,
    destructiveLight: RosePrimitives.p300Dark,
    value: GoldPrimitives.p500Dark,
    brand: BrandPrimitives.p400Dark,
  );

  static const light = AppIconColors(
    accent: Primitives.p900Light,
    regular: Primitives.p700Light,
    muted: Primitives.p500Light,
    disabled: Primitives.p300Light,
    inverse: Primitives.p0Light,
    onPrimary: Primitives.p0Light,
    warning: OrangePrimitives.p400Light,
    destructive: RosePrimitives.p300Light,
    destructiveLight: RosePrimitives.p200Light,
    value: GoldPrimitives.p400Light,
    brand: BrandPrimitives.p300Light,
  );
}

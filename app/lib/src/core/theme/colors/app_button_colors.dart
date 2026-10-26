import 'package:flutter/painting.dart';

import '../primitives.dart';

/// Button colors grouped by variant.
///
/// Each variant owns its own sub-palette so widgets reference them as
/// `button.primary.bg`, `button.ghost.bgHover`, etc.
class AppButtonColors {
  const AppButtonColors({
    required this.primary,
    required this.secondary,
    required this.ghost,
    required this.disabled,
    required this.destructive,
  });

  final AppPrimaryButtonColors primary;
  final AppSecondaryButtonColors secondary;
  final AppGhostButtonColors ghost;
  final AppDisabledButtonColors disabled;
  final AppDestructiveButtonColors destructive;

  static const dark = AppButtonColors(
    primary: AppPrimaryButtonColors.dark,
    secondary: AppSecondaryButtonColors.dark,
    ghost: AppGhostButtonColors.dark,
    disabled: AppDisabledButtonColors.dark,
    destructive: AppDestructiveButtonColors.dark,
  );

  static const light = AppButtonColors(
    primary: AppPrimaryButtonColors.light,
    secondary: AppSecondaryButtonColors.light,
    ghost: AppGhostButtonColors.light,
    disabled: AppDisabledButtonColors.light,
    destructive: AppDestructiveButtonColors.light,
  );
}

class AppPrimaryButtonColors {
  const AppPrimaryButtonColors({
    required this.bg,
    required this.bgHover,
    required this.bgPressed,
    required this.border,
    required this.borderHover,
    required this.borderPressed,
    required this.label,
    required this.labelHover,
  });

  final Color bg;
  final Color bgHover;
  final Color bgPressed;
  final Color border;
  final Color borderHover;
  final Color borderPressed;
  final Color label;
  final Color labelHover;

  // Zafe (Verdigris): a light teal fill with a deep teal label in dark mode, a deep
  // teal fill with a white label in light mode (docs/brand.md §3).
  static const dark = AppPrimaryButtonColors(
    bg: BrandPrimitives.p400Dark,
    bgHover: BrandPrimitives.p500Dark,
    bgPressed: BrandPrimitives.p500Dark,
    border: Primitives.p900Alpha10Dark,
    borderHover: Primitives.p900Alpha10Dark,
    borderPressed: Primitives.p900Alpha10Dark,
    label: BrandPrimitives.p50Dark,
    labelHover: BrandPrimitives.p50Dark,
  );

  static const light = AppPrimaryButtonColors(
    bg: BrandPrimitives.p300Light,
    bgHover: BrandPrimitives.p400Light,
    bgPressed: BrandPrimitives.p400Light,
    border: Primitives.p900Alpha10Light,
    borderHover: Primitives.p900Alpha10Light,
    borderPressed: Primitives.p900Alpha10Light,
    // theme-pair: the fill flips (light teal / deep teal), so the label does too.
    label: Primitives.p0Light,
    // theme-pair: as label.
    labelHover: Primitives.p0Light,
  );
}

class AppSecondaryButtonColors {
  const AppSecondaryButtonColors({
    required this.bg,
    required this.bgHover,
    required this.bgPressed,
    required this.label,
  });

  final Color bg;
  final Color bgHover;
  final Color bgPressed;
  final Color label;

  static const dark = AppSecondaryButtonColors(
    bg: Primitives.p150Dark,
    bgHover: Primitives.p200Dark,
    bgPressed: Primitives.p200Dark,
    label: Primitives.p800Dark,
  );

  static const light = AppSecondaryButtonColors(
    bg: Primitives.p0Light,
    bgHover: Primitives.p100Light,
    bgPressed: Primitives.p100Light,
    label: Primitives.p900Light,
  );
}

class AppGhostButtonColors {
  const AppGhostButtonColors({
    required this.bg,
    required this.bgHover,
    required this.border,
    required this.label,
  });

  // Transparent-looking base; the concrete token equals ground so the fill
  // reads as "no fill" against Scaffold.
  final Color bg;
  final Color bgHover;
  final Color border;
  final Color label;

  static const dark = AppGhostButtonColors(
    bg: Primitives.p0Dark,
    // p100Dark equals the raised/modal surface (#232828), which made the
    // hover fill invisible on modal panels — one step up stays visible on
    // both ground and raised surfaces.
    bgHover: Primitives.p150Dark,
    border: Primitives.p300Dark,
    label: Primitives.p700Dark,
  );

  static const light = AppGhostButtonColors(
    bg: Primitives.p0Light,
    bgHover: Primitives.p100Light,
    border: Primitives.p300Light,
    label: Primitives.p800Light,
  );
}

class AppDisabledButtonColors {
  const AppDisabledButtonColors({required this.bg, required this.label});

  final Color bg;
  final Color label;

  static const dark = AppDisabledButtonColors(
    bg: Primitives.p300Alpha20Dark,
    label: Primitives.p500Alpha50Dark,
  );

  static const light = AppDisabledButtonColors(
    bg: Primitives.p300Alpha20Light,
    label: Primitives.p500Alpha50Light,
  );
}

class AppDestructiveButtonColors {
  const AppDestructiveButtonColors({
    required this.bg,
    required this.bgHover,
    required this.bgPressed,
    required this.border,
    required this.borderHover,
    required this.borderPressed,
    required this.label,
  });

  final Color bg;
  final Color bgHover;
  final Color bgPressed;
  final Color border;
  final Color borderHover;
  final Color borderPressed;
  final Color label;

  static const dark = AppDestructiveButtonColors(
    bg: RosePrimitives.p200Dark,
    bgHover: RosePrimitives.p150Dark,
    bgPressed: RosePrimitives.p150Dark,
    border: Primitives.p900Alpha10Dark,
    borderHover: Primitives.p900Alpha10Dark,
    borderPressed: Primitives.p900Alpha10Dark,
    label: RosePrimitives.p800Dark,
  );

  static const light = AppDestructiveButtonColors(
    bg: RosePrimitives.p500Light,
    bgHover: RosePrimitives.p600Light,
    bgPressed: RosePrimitives.p600Light,
    border: Primitives.p900Alpha5Light,
    borderHover: Primitives.p900Alpha5Light,
    borderPressed: Primitives.p900Alpha5Light,
    label: RosePrimitives.p50Light,
  );
}

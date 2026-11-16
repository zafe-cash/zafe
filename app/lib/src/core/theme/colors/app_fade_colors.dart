import 'package:flutter/painting.dart';

import '../primitives.dart';

class AppFadeColors {
  const AppFadeColors({required this.illustration});

  final Color illustration;

  static const dark = AppFadeColors(illustration: Primitives.p0Alpha50Dark);

  static const light = AppFadeColors(illustration: Primitives.p0Alpha0Dark);
}

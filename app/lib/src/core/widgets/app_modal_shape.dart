import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';

ShapeBorder appModalShape(BorderRadius borderRadius) =>
    defaultTargetPlatform == TargetPlatform.iOS
    ? RoundedSuperellipseBorder(borderRadius: borderRadius)
    : RoundedRectangleBorder(borderRadius: borderRadius);

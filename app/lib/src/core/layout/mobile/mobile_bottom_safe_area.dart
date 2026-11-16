import 'package:flutter/foundation.dart'
    show TargetPlatform, defaultTargetPlatform;
import 'package:flutter/widgets.dart';

const double kIosHomeIndicatorClearance = 16;

class MobileBottomSafeArea extends StatelessWidget {
  const MobileBottomSafeArea({
    required this.bottomPadding,
    required this.child,
    super.key,
  });

  final double bottomPadding;

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final skipInset =
        defaultTargetPlatform == TargetPlatform.iOS &&
        bottomPadding >= kIosHomeIndicatorClearance;
    return SafeArea(top: false, bottom: !skipInset, child: child);
  }
}

import 'package:flutter/foundation.dart'
    show TargetPlatform, defaultTargetPlatform;
import 'package:flutter/material.dart' show Scaffold;
import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';
import 'mobile_bottom_safe_area.dart';

class AppMobileShell extends StatelessWidget {
  const AppMobileShell({required this.body, required this.tabBar, super.key});

  final Widget body;
  final Widget tabBar;

  @override
  Widget build(BuildContext context) {
    final bottomGap = defaultTargetPlatform == TargetPlatform.iOS
        ? AppSpacing.sm
        : AppSpacing.s;
    return Scaffold(
      backgroundColor: context.colors.background.window,
      extendBody: true,
      body: body,
      bottomNavigationBar: MobileBottomSafeArea(
        bottomPadding: bottomGap,
        child: Padding(
          padding: EdgeInsets.fromLTRB(
            AppSpacing.sm,
            0,
            AppSpacing.sm,
            bottomGap,
          ),
          child: tabBar,
        ),
      ),
    );
  }
}

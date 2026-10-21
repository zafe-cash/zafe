import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../theme/app_theme.dart';
import 'mobile_top_nav.dart';

/// Pushed full-screen route: back nav, scrollable body, pinned CTA area.
class ZafeScreen extends StatelessWidget {
  const ZafeScreen({
    super.key,
    required this.title,
    required this.children,
    this.bottom,
    this.onBack,
    this.showBack = true,
  });

  final String title;
  final List<Widget> children;
  final Widget? bottom;
  final VoidCallback? onBack;
  final bool showBack;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        child: Column(
          children: [
            MobileTopNav.back(
              title: title,
              onBack: showBack ? (onBack ?? () => context.pop()) : null,
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: children,
              ),
            ),
            if (bottom != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                // Full width, so `AppButton(expand: true)` spans the screen.
                child: SizedBox(width: double.infinity, child: bottom),
              ),
          ],
        ),
      ),
    );
  }
}

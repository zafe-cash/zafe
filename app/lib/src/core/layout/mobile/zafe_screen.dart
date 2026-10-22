import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../theme/app_theme.dart';
import 'mobile_top_nav.dart';

/// Pushed full-screen route: back nav, scrollable body, pinned CTA area. In a short
/// window (landscape, a flip phone's cover screen) the CTA area scrolls with the body
/// instead, so the body keeps room to show.
class ZafeScreen extends StatefulWidget {
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

  /// Below this much height for the body and the CTA area (keyboard closed), the CTA
  /// area scrolls.
  static const double pinBottomHeight = 360;

  @override
  State<ZafeScreen> createState() => _ZafeScreenState();
}

class _ZafeScreenState extends State<ZafeScreen> {
  /// The CTA area keeps its state (a button's spinner) when it moves.
  final _bottomKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final bottom = widget.bottom == null
        ? null
        : KeyedSubtree(
            key: _bottomKey,
            // Full width, so `AppButton(expand: true)` spans the screen.
            child: SizedBox(width: double.infinity, child: widget.bottom),
          );
    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        child: Column(
          children: [
            MobileTopNav.back(
              title: widget.title,
              onBack: widget.showBack
                  ? (widget.onBack ?? () => context.pop())
                  : null,
            ),
            Expanded(
              child: LayoutBuilder(
                builder: (context, box) {
                  // Measured without the keyboard: above it, the CTA stays pinned
                  // as on any phone.
                  final pinned =
                      bottom != null &&
                      box.maxHeight + MediaQuery.viewInsetsOf(context).bottom >=
                          ZafeScreen.pinBottomHeight;
                  return Column(
                    children: [
                      Expanded(
                        child: ListView(
                          padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                          children: [
                            ...widget.children,
                            if (bottom != null && !pinned) bottom,
                          ],
                        ),
                      ),
                      if (pinned)
                        Padding(
                          padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                          child: bottom,
                        ),
                    ],
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}

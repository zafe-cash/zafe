import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../theme/app_theme.dart';
import 'mobile_top_nav.dart';

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

  static const double pinBottomHeight = 360;

  @override
  State<ZafeScreen> createState() => _ZafeScreenState();
}

class _ZafeScreenState extends State<ZafeScreen> {
  final _bottomKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final bottom = widget.bottom == null
        ? null
        : KeyedSubtree(
            key: _bottomKey,
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

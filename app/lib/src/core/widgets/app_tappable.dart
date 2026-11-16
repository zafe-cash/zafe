import 'package:flutter/widgets.dart';

class AppTappable extends StatelessWidget {
  const AppTappable({
    required this.onTap,
    required this.child,
    this.semanticsLabel,
    this.enabled = true,
    this.behavior = HitTestBehavior.opaque,
    super.key,
  });

  final VoidCallback? onTap;
  final Widget child;
  final String? semanticsLabel;
  final bool enabled;
  final HitTestBehavior behavior;

  @override
  Widget build(BuildContext context) {
    final active = enabled && onTap != null;
    return Semantics(
      button: true,
      enabled: active,
      label: semanticsLabel,
      child: MouseRegion(
        cursor: active ? SystemMouseCursors.click : SystemMouseCursors.basic,
        child: GestureDetector(
          behavior: behavior,
          onTap: active ? onTap : null,
          child: child,
        ),
      ),
    );
  }
}

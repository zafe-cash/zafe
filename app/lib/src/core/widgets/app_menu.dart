import 'package:flutter/material.dart'
    show
        ButtonStyle,
        MenuAnchor,
        MenuController,
        MenuItemButton,
        MenuStyle,
        WidgetStatePropertyAll;
import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';
import 'app_icon.dart';

/// One choice in an [AppMenu].
class AppMenuItem {
  const AppMenuItem({
    required this.label,
    required this.onTap,
    this.icon,
    this.selected = false,
    this.destructive = false,
  });

  final String label;
  final VoidCallback onTap;
  final String? icon;

  /// The current value of a setting (a check after the label).
  final bool selected;
  final bool destructive;
}

/// A short list of choices anchored to what opened it (a row's value, a ⋯): picking one
/// acts at once, no Update button and no sheet. For a handful of short options; anything
/// with typing or a second step is a pushed screen.
class AppMenu extends StatelessWidget {
  const AppMenu({
    super.key,
    required this.items,
    required this.builder,
  });

  final List<AppMenuItem> items;

  /// The anchor; call `open` to show the menu.
  final Widget Function(BuildContext context, VoidCallback open) builder;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MenuAnchor(
      alignmentOffset: const Offset(0, AppSpacing.xxs),
      style: MenuStyle(
        backgroundColor: WidgetStatePropertyAll(colors.background.base),
        surfaceTintColor: const WidgetStatePropertyAll(Color(0x00000000)),
        elevation: const WidgetStatePropertyAll(8),
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(vertical: AppSpacing.xs),
        ),
        shape: WidgetStatePropertyAll(
          RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(AppRadii.large),
            side: BorderSide(color: colors.border.subtle),
          ),
        ),
      ),
      menuChildren: [
        for (final item in items)
          MenuItemButton(
            onPressed: item.onTap,
            style: const ButtonStyle(
              minimumSize: WidgetStatePropertyAll(Size(200, 48)),
              padding: WidgetStatePropertyAll(
                EdgeInsets.symmetric(horizontal: AppSpacing.sm),
              ),
            ),
            leadingIcon: item.icon == null
                ? null
                : AppIcon(
                    item.icon!,
                    size: 20,
                    color: item.destructive
                        ? colors.icon.destructive
                        : colors.icon.muted,
                    patina: item.destructive ? colors.icon.destructive : null,
                  ),
            trailingIcon: item.selected
                ? AppIcon(AppIcons.check, size: 18, color: colors.icon.accent)
                : null,
            child: Text(
              item.label,
              style: AppTypography.labelLarge.copyWith(
                fontWeight: FontWeight.w400,
                color: item.destructive
                    ? colors.text.destructive
                    : colors.text.accent,
              ),
            ),
          ),
      ],
      builder: (context, controller, _) => builder(
        context,
        () => _toggle(controller),
      ),
    );
  }

  static void _toggle(MenuController controller) =>
      controller.isOpen ? controller.close() : controller.open();
}

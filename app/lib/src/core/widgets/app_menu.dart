import 'dart:math' as math;

import 'package:flutter/material.dart'
    show ButtonStyle, MenuController, MenuItemButton, WidgetStatePropertyAll;
import 'package:flutter/services.dart';
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
///
/// Placed by [AppMenuLayout], not MenuAnchor: MenuAnchor starts a menu at the anchor
/// and, when it would run off screen, flips it to the other side or pins it to the
/// screen edge with no margin, so a ⋯ near the right edge got a menu hanging past the
/// page margin, and a narrow phone at large text one off the left edge.
class AppMenu extends StatefulWidget {
  const AppMenu({super.key, required this.items, required this.builder});

  final List<AppMenuItem> items;

  /// The anchor; call `open` to show the menu.
  final Widget Function(BuildContext context, VoidCallback open) builder;

  @override
  State<AppMenu> createState() => _AppMenuState();
}

class _AppMenuState extends State<AppMenu> {
  final _controller = MenuController();

  /// While open, Back closes the menu instead of leaving the screen (neither
  /// MenuAnchor nor RawMenuAnchor handles Back).
  bool _open = false;

  static const _shortcuts = <ShortcutActivator, Intent>{
    SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
    SingleActivator(LogicalKeyboardKey.arrowDown): DirectionalFocusIntent(
      TraversalDirection.down,
    ),
    SingleActivator(LogicalKeyboardKey.arrowUp): DirectionalFocusIntent(
      TraversalDirection.up,
    ),
  };

  void _toggle() =>
      _controller.isOpen ? _controller.close() : _controller.open();

  @override
  Widget build(BuildContext context) {
    return RawMenuAnchor(
      controller: _controller,
      onOpen: () => setState(() => _open = true),
      onClose: () {
        if (mounted) setState(() => _open = false);
      },
      overlayBuilder: (context, info) {
        final media = MediaQuery.of(context);
        return Positioned.fill(
          child: CustomSingleChildLayout(
            delegate: AppMenuLayout(
              anchor: info.anchorRect,
              // Status bar, navigation bar and an open keyboard are off limits.
              safe:
                  media.padding +
                  EdgeInsets.only(bottom: media.viewInsets.bottom),
              textDirection: Directionality.of(context),
            ),
            child: Semantics(
              scopesRoute: true,
              explicitChildNodes: true,
              child: TapRegion(
                groupId: info.tapRegionGroupId,
                onTapOutside: (_) => _controller.close(),
                child: Actions(
                  actions: {
                    DismissIntent: CallbackAction<DismissIntent>(
                      onInvoke: (_) => _controller.close(),
                    ),
                  },
                  child: Shortcuts(
                    shortcuts: _shortcuts,
                    child: FocusScope(child: _panel(context)),
                  ),
                ),
              ),
            ),
          ),
        );
      },
      builder: (context, controller, _) => PopScope(
        canPop: !_open,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop) _controller.close();
        },
        child: widget.builder(context, _toggle),
      ),
    );
  }

  Widget _panel(BuildContext context) {
    final colors = context.colors;
    return IntrinsicWidth(
      child: Container(
        clipBehavior: Clip.antiAlias,
        constraints: const BoxConstraints(minWidth: 200),
        padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
        decoration: BoxDecoration(
          color: colors.background.base,
          borderRadius: BorderRadius.circular(AppRadii.large),
          border: Border.all(color: colors.border.subtle),
          boxShadow: const [
            BoxShadow(
              color: Color(0x29000000),
              blurRadius: 24,
              offset: Offset(0, 8),
            ),
          ],
        ),
        // Taller than the screen allows (many items at 2x text): it scrolls.
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [for (final item in widget.items) _item(colors, item)],
          ),
        ),
      ),
    );
  }

  Widget _item(AppColors colors, AppMenuItem item) => MenuItemButton(
    onPressed: () {
      _controller.close();
      item.onTap();
    },
    // A label longer than the widest menu the screen allows wraps.
    overflowAxis: Axis.vertical,
    style: const ButtonStyle(
      minimumSize: WidgetStatePropertyAll(Size(0, 48)),
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
        color: item.destructive ? colors.text.destructive : colors.text.accent,
      ),
    ),
  );
}

/// Where an [AppMenu] goes: hanging from the anchor's trailing edge (a ⋯ or a row's
/// value sits at the end of its row), [gap] below it, or above it when there is no room
/// below; never wider than the screen less a [margin] each side, and moved inside those
/// margins rather than off screen, on any screen and at any text size.
class AppMenuLayout extends SingleChildLayoutDelegate {
  const AppMenuLayout({
    required this.anchor,
    required this.safe,
    required this.textDirection,
    this.margin = AppSpacing.sm,
    this.gap = AppSpacing.xxs,
  });

  final Rect anchor;
  final EdgeInsets safe;
  final TextDirection textDirection;
  final double margin;
  final double gap;

  Rect _area(Size size) => Rect.fromLTRB(
    safe.left + margin,
    safe.top + margin,
    size.width - safe.right - margin,
    size.height - safe.bottom - margin,
  );

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    final area = _area(constraints.biggest);
    return BoxConstraints.loose(
      Size(math.max(0.0, area.width), math.max(0.0, area.height)),
    );
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final area = _area(size);
    final hang = textDirection == TextDirection.ltr
        ? anchor.right - childSize.width
        : anchor.left;
    final double x = hang
        .clamp(area.left, math.max(area.left, area.right - childSize.width))
        .toDouble();
    final below = anchor.bottom + gap;
    final above = anchor.top - gap - childSize.height;
    final double y;
    if (below + childSize.height <= area.bottom) {
      y = below;
    } else if (above >= area.top) {
      y = above;
    } else {
      // Fits neither way: as low as it goes, inside the margins.
      y = math.max(area.top, area.bottom - childSize.height);
    }
    return Offset(x, y);
  }

  @override
  bool shouldRelayout(AppMenuLayout old) =>
      anchor != old.anchor ||
      safe != old.safe ||
      textDirection != old.textDirection ||
      margin != old.margin ||
      gap != old.gap;
}

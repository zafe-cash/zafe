import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';

enum AppButtonVariant { primary, secondary, ghost, destructive }

enum AppButtonSize { large, mediumLarge, medium, small }

class _Sizing {
  const _Sizing({
    required this.height,
    required this.padding,
    required this.gap,
    required this.iconSize,
    required this.labelStyle,
  });

  final double height;
  final EdgeInsets padding;
  final double gap;
  final double iconSize;
  final TextStyle labelStyle;
}

const _largeSizing = _Sizing(
  height: AppButtonSizing.largeHeight,
  padding: EdgeInsets.symmetric(
    horizontal: AppSpacing.sm,
    vertical: AppSpacing.xs,
  ),
  gap: AppSpacing.xxs,
  iconSize: 20,
  labelStyle: AppTypography.labelLarge,
);

const _mediumLargeSizing = _Sizing(
  height: 36,
  padding: EdgeInsets.symmetric(
    horizontal: AppSpacing.s,
    vertical: AppSpacing.xxs,
  ),
  gap: AppSpacing.xxs,
  iconSize: AppIconSize.medium,
  labelStyle: AppTypography.labelLarge,
);

const _mediumSizing = _Sizing(
  height: 32,
  padding: EdgeInsets.symmetric(
    horizontal: AppSpacing.xs,
    vertical: AppSpacing.xxs,
  ),
  gap: AppSpacing.xxs,
  iconSize: AppButtonSizing.mediumSmallIconSize,
  labelStyle: AppTypography.labelLarge,
);

const _smallSizing = _Sizing(
  height: 24,
  padding: EdgeInsets.symmetric(horizontal: AppSpacing.xxs),
  gap: AppSpacing.xxs,
  iconSize: AppButtonSizing.mediumSmallIconSize,
  labelStyle: AppTypography.labelSmall,
);

class _VariantPalette {
  const _VariantPalette({
    required this.bg,
    required this.bgHover,
    required this.bgPressed,
    required this.border,
    required this.borderHover,
    required this.borderPressed,
    required this.borderWidth,
    required this.label,
    required this.labelHover,
    required this.focusRing,
    required this.focusRingHover,
  });

  final Color bg;
  final Color bgHover;
  final Color bgPressed;
  final Color border;
  final Color borderHover;
  final Color borderPressed;
  final double borderWidth;
  final Color label;
  final Color labelHover;
  final Color focusRing;
  final Color focusRingHover;
}

_VariantPalette _paletteFor(AppButtonVariant variant, AppColors c) {
  switch (variant) {
    case AppButtonVariant.primary:
      return _VariantPalette(
        bg: c.button.primary.bg,
        bgHover: c.button.primary.bgHover,
        bgPressed: c.button.primary.bgPressed,
        border: c.button.primary.border,
        borderHover: c.button.primary.borderHover,
        borderPressed: c.button.primary.borderPressed,
        borderWidth: 1.5,
        label: c.button.primary.label,
        labelHover: c.button.primary.labelHover,
        focusRing: c.button.primary.bg,
        focusRingHover: c.button.primary.bgHover,
      );
    case AppButtonVariant.secondary:
      return _VariantPalette(
        bg: c.button.secondary.bg,
        bgHover: c.button.secondary.bgHover,
        bgPressed: c.button.secondary.bgPressed,
        border: c.background.ground.withValues(alpha: 0),
        borderHover: c.background.ground.withValues(alpha: 0),
        borderPressed: c.background.ground.withValues(alpha: 0),
        borderWidth: 0,
        label: c.button.secondary.label,
        labelHover: c.button.secondary.label,
        focusRing: c.state.focusRing,
        focusRingHover: c.state.focusRing,
      );
    case AppButtonVariant.ghost:
      return _VariantPalette(
        bg: c.background.ground.withValues(alpha: 0),
        bgHover: c.button.ghost.bgHover,
        bgPressed: c.button.ghost.bgHover,
        border: c.background.ground.withValues(alpha: 0),
        borderHover: c.background.ground.withValues(alpha: 0),
        borderPressed: c.background.ground.withValues(alpha: 0),
        borderWidth: 0,
        label: c.button.ghost.label,
        labelHover: c.button.ghost.label,
        focusRing: c.state.focusRing,
        focusRingHover: c.state.focusRing,
      );
    case AppButtonVariant.destructive:
      return _VariantPalette(
        bg: c.button.destructive.bg,
        bgHover: c.button.destructive.bgHover,
        bgPressed: c.button.destructive.bgPressed,
        border: c.button.destructive.border,
        borderHover: c.button.destructive.borderHover,
        borderPressed: c.button.destructive.borderPressed,
        borderWidth: 1.5,
        label: c.button.destructive.label,
        labelHover: c.button.destructive.label,
        focusRing: c.state.focusRingDestructive,
        focusRingHover: c.state.focusRingDestructive,
      );
  }
}

class AppButton extends StatefulWidget {
  const AppButton({
    super.key,
    required this.onPressed,
    required this.child,
    this.variant = AppButtonVariant.primary,
    this.size = AppButtonSize.large,
    this.height,
    this.growWithContent = false,
    this.contentPadding,
    this.borderRadius,
    this.leading,
    this.trailing,
    this.minWidth,
    this.iconGap,
    this.focusRingColor,
    this.disabledBackgroundColor,
    this.enabledBackgroundColor,
    this.pressedBackgroundColor,
    this.enabledLabelColor,
    this.pressedLabelColor,
    this.enabledBorderColor,
    this.focusNode,
    this.autofocus = false,
    this.expand = false,
    this.constrainContent = false,
  });

  final VoidCallback? onPressed;

  final Widget child;

  final AppButtonVariant variant;
  final AppButtonSize size;

  final double? height;

  final bool growWithContent;

  final EdgeInsets? contentPadding;

  final BorderRadius? borderRadius;

  final Widget? leading;

  final Widget? trailing;

  final double? minWidth;

  final double? iconGap;

  final Color? focusRingColor;

  final Color? disabledBackgroundColor;

  final Color? enabledBackgroundColor;
  final Color? pressedBackgroundColor;
  final Color? enabledLabelColor;
  final Color? pressedLabelColor;

  final Color? enabledBorderColor;

  final FocusNode? focusNode;
  final bool autofocus;

  final bool expand;

  final bool constrainContent;

  @override
  State<AppButton> createState() => _AppButtonState();
}

class _AppButtonState extends State<AppButton> {
  bool _hovered = false;
  bool _pressed = false;
  bool _focused = false;

  bool get _enabled => widget.onPressed != null;

  void _setHovered(bool value) {
    if (_hovered != value) setState(() => _hovered = value);
  }

  void _setPressed(bool value) {
    if (_pressed != value) setState(() => _pressed = value);
  }

  void _handleFocusChange(bool value) {
    if (_focused != value) setState(() => _focused = value);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final palette = _paletteFor(widget.variant, colors);
    final sizing = switch (widget.size) {
      AppButtonSize.large => _largeSizing,
      AppButtonSize.mediumLarge => _mediumLargeSizing,
      AppButtonSize.medium => _mediumSizing,
      AppButtonSize.small => _smallSizing,
    };
    final height = widget.height ?? sizing.height;
    final isGhost = widget.variant == AppButtonVariant.ghost;

    final disabled = colors.button.disabled;

    final Color currentBg = !_enabled
        ? widget.disabledBackgroundColor ?? disabled.bg
        : _pressed
        ? widget.pressedBackgroundColor ?? palette.bgPressed
        : _hovered
        ? widget.pressedBackgroundColor ?? palette.bgHover
        : widget.enabledBackgroundColor ?? palette.bg;

    final Color labelColor = !_enabled
        ? disabled.label
        : _pressed || _hovered
        ? widget.pressedLabelColor ?? palette.labelHover
        : widget.enabledLabelColor ?? palette.label;
    final Color stateBorderColor = _pressed
        ? palette.borderPressed
        : _hovered
        ? palette.borderHover
        : palette.border;
    final Color borderColor = !_enabled
        ? palette.border
        : widget.enabledBorderColor ?? stateBorderColor;
    final borderWidth = _enabled ? palette.borderWidth : 0.0;
    final iconGap = widget.iconGap ?? sizing.gap;
    final contentPadding = widget.contentPadding ?? sizing.padding;
    final radius =
        widget.borderRadius ?? BorderRadius.circular(zafeButtonRadius(height));
    final OutlinedBorder shape = RoundedRectangleBorder(borderRadius: radius);

    final rowChildren = <Widget>[];
    if (widget.leading != null) {
      rowChildren
        ..add(
          SizedBox(
            width: sizing.iconSize,
            height: sizing.iconSize,
            child: widget.leading,
          ),
        )
        ..add(SizedBox(width: iconGap));
    }
    final label = DefaultTextStyle.merge(
      style: sizing.labelStyle.copyWith(color: labelColor),
      child: widget.child,
    );
    final labelSlot = widget.size == AppButtonSize.small
        ? label
        : Padding(
            padding: const EdgeInsets.symmetric(horizontal: AppSpacing.xxs),
            child: label,
          );
    final labelIndex = rowChildren.length;
    rowChildren.add(const SizedBox.shrink());
    if (widget.trailing != null) {
      rowChildren
        ..add(SizedBox(width: iconGap))
        ..add(
          SizedBox(
            width: sizing.iconSize,
            height: sizing.iconSize,
            child: widget.trailing,
          ),
        );
    }

    final pill = ConstrainedBox(
      constraints: BoxConstraints(
        minWidth: widget.minWidth ?? 0,
        minHeight: widget.growWithContent ? height : 0,
      ),
      child: AnimatedContainer(
        duration: isGhost ? Duration.zero : const Duration(milliseconds: 120),
        curve: Curves.easeOut,
        height: widget.growWithContent ? null : height,
        decoration: ShapeDecoration(
          color: currentBg,
          shape: shape.copyWith(
            side: borderWidth == 0
                ? BorderSide.none
                : BorderSide(color: borderColor, width: borderWidth),
          ),
        ),
        padding: contentPadding,
        child: IconTheme.merge(
          data: IconThemeData(color: labelColor, size: sizing.iconSize),
          child: LayoutBuilder(
            builder: (context, constraints) {
              final children = [...rowChildren];
              children[labelIndex] =
                  widget.constrainContent || constraints.hasBoundedWidth
                  ? Flexible(
                      fit: FlexFit.loose,
                      child: FittedBox(fit: BoxFit.scaleDown, child: labelSlot),
                    )
                  : labelSlot;
              return Row(
                mainAxisSize: widget.constrainContent
                    ? MainAxisSize.max
                    : MainAxisSize.min,
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.center,
                children: children,
              );
            },
          ),
        ),
      ),
    );

    final focusRingWidth = widget.size == AppButtonSize.small ? 1.5 : 2.0;
    final focusRingColor =
        widget.focusRingColor ??
        (_hovered ? palette.focusRingHover : palette.focusRing);
    final focusRingOutset = switch (widget.variant) {
      AppButtonVariant.primary =>
        widget.size == AppButtonSize.large ? 3.5 : 3.0,
      AppButtonVariant.destructive => 3.5,
      AppButtonVariant.secondary || AppButtonVariant.ghost => 2.0,
    };
    final OutlinedBorder focusShape = RoundedRectangleBorder(
      borderRadius: radius + BorderRadius.circular(focusRingOutset),
    );

    final focusShell = Stack(
      clipBehavior: Clip.none,
      alignment: Alignment.center,
      fit: widget.expand ? StackFit.passthrough : StackFit.loose,
      children: [
        pill,
        Positioned(
          left: -focusRingOutset,
          top: -focusRingOutset,
          right: -focusRingOutset,
          bottom: -focusRingOutset,
          child: IgnorePointer(
            child: AnimatedOpacity(
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              opacity: (_focused && _enabled) ? 1.0 : 0.0,
              child: DecoratedBox(
                decoration: ShapeDecoration(
                  shape: focusShape.copyWith(
                    side: BorderSide(
                      color: focusRingColor,
                      width: focusRingWidth,
                      strokeAlign: BorderSide.strokeAlignOutside,
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );

    final pointer = MouseRegion(
      cursor: _enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: _enabled ? (_) => _setHovered(true) : null,
      onExit: _enabled ? (_) => _setHovered(false) : null,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        excludeFromSemantics: true,
        onTapDown: _enabled ? (_) => _setPressed(true) : null,
        onTapUp: _enabled
            ? (_) {
                _setPressed(false);
                widget.onPressed!.call();
              }
            : null,
        onTapCancel: _enabled ? () => _setPressed(false) : null,
        child: focusShell,
      ),
    );

    return MergeSemantics(
      child: Semantics(
        button: true,
        enabled: _enabled,
        onTap: _enabled ? widget.onPressed : null,
        child: Focus(
          focusNode: widget.focusNode,
          autofocus: widget.autofocus,
          canRequestFocus: _enabled,
          onFocusChange: _handleFocusChange,
          onKeyEvent: _handleKeyEvent,
          child: AnimatedScale(
            scale: _pressed && _enabled ? 0.96 : 1,
            duration: const Duration(milliseconds: 90),
            curve: Curves.easeOut,
            child: pointer,
          ),
        ),
      ),
    );
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (!_enabled) return KeyEventResult.ignored;
    final key = event.logicalKey;
    final isActivate =
        key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter ||
        key == LogicalKeyboardKey.space;
    if (!isActivate) return KeyEventResult.ignored;

    if (event is KeyDownEvent) {
      _setPressed(true);
      return KeyEventResult.handled;
    }
    if (event is KeyUpEvent) {
      if (_pressed) {
        _setPressed(false);
        widget.onPressed?.call();
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.handled;
  }
}

double zafeButtonRadius(double height) =>
    height >= 44 ? 14 : (height >= 32 ? 10 : 8);

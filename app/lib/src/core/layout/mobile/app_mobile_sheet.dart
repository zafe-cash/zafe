import 'dart:ui' as ui;

import 'package:flutter/foundation.dart'
    show TargetPlatform, defaultTargetPlatform;
import 'package:flutter/material.dart'
    show Material, MaterialLocalizations, showModalBottomSheet;
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../theme/app_theme.dart';
import '../../widgets/app_icon.dart';
import '../../widgets/app_modal_shape.dart';
import '../app_column.dart';
import 'mobile_modal_corners.dart';
import 'prepared_modal_sheet_route.dart';

Future<T?> showAppMobileSheet<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  bool isDismissible = true,
  bool enableDrag = true,
  bool transparentBackground = false,
}) {
  final appTheme = context.appTheme;
  final colors = context.colors;
  ProviderContainer? providerContainer;
  try {
    providerContainer = ProviderScope.containerOf(context, listen: false);
  } on StateError {
    providerContainer = null;
  }

  Widget wrapSheet(Widget child) {
    final themed = AppTheme(data: appTheme, child: child);
    final container = providerContainer;
    if (container == null) return themed;
    return UncontrolledProviderScope(container: container, child: themed);
  }

  if (defaultTargetPlatform == TargetPlatform.iOS && !transparentBackground) {
    final navigator = Navigator.of(context, rootNavigator: true);
    final localizations = MaterialLocalizations.of(context);
    return navigator.push(
      PreparedModalSheetRoute<T>(
        capturedThemes: InheritedTheme.capture(
          from: context,
          to: navigator.context,
        ),
        modalBarrierColor: colors.background.neutralScrim,
        barrierLabel: localizations.scrimLabel,
        barrierOnTapHint: localizations.scrimOnTapHint(
          localizations.bottomSheetLabel,
        ),
        isDismissible: isDismissible,
        enableDrag: enableDrag,
        builder: (_) => wrapSheet(
          Builder(
            builder: (context) => MobileModalCard(child: builder(context)),
          ),
        ),
      ),
    );
  }

  return showModalBottomSheet<T>(
    context: context,
    isDismissible: isDismissible,
    enableDrag: enableDrag,
    isScrollControlled: true,
    useSafeArea: true,
    useRootNavigator: true,
    backgroundColor: const Color(0x00000000),
    elevation: 0,
    constraints: const BoxConstraints(maxWidth: kMaxContentWidth),
    barrierColor: colors.background.neutralScrim,
    builder: (sheetContext) => wrapSheet(
      Builder(
        builder: (themedContext) => MobileModalCard(
          transparentBackground: transparentBackground,
          child: builder(themedContext),
        ),
      ),
    ),
  );
}

class MobileModalCard extends StatelessWidget {
  const MobileModalCard({
    required this.child,
    this.transparentBackground = false,
    this.margin,
    this.followsScreenCorners = true,
    super.key,
  });

  final Widget child;

  final bool transparentBackground;

  final EdgeInsets? margin;

  final bool followsScreenCorners;

  static double bottomGapFor(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    final keyboardInset = mediaQuery.viewInsets.bottom;

    final double bottomGap;
    if (keyboardInset > 0) {
      bottomGap = keyboardInset + AppSpacing.sm;
    } else if (defaultTargetPlatform == TargetPlatform.iOS) {
      bottomGap = AppSpacing.sm;
    } else {
      bottomGap = AppSpacing.sm + mediaQuery.viewPadding.bottom;
    }
    return bottomGap;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;

    final ios = defaultTargetPlatform == TargetPlatform.iOS;
    Widget surface(BorderRadius radius) {
      final shape = appModalShape(radius);
      return DecoratedBox(
        decoration: ios
            ? ShapeDecoration(shape: shape, shadows: _modalShadow)
            : BoxDecoration(borderRadius: radius, boxShadow: _modalShadow),
        child: Material(
          color: colors.background.base,
          clipBehavior: Clip.antiAlias,
          shape: shape,
          child: CustomPaint(
            foregroundPainter: _ModalInnerHighlightPainter(shape),
            child: child,
          ),
        ),
      );
    }

    final Widget card = transparentBackground
        ? child
        : ios
        ? MobileModalCorners(
            followsScreenCorners: followsScreenCorners,
            builder: (_, radius) => surface(radius),
          )
        : surface(const BorderRadius.all(Radius.circular(AppRadii.xLarge)));

    final double sideMargin = transparentBackground ? 0 : AppSpacing.sm;
    return Padding(
      padding:
          margin ??
          EdgeInsets.only(
            left: sideMargin,
            right: sideMargin,
            bottom: bottomGapFor(context),
          ),
      child: card,
    );
  }
}

class MobileModalOverlay extends StatelessWidget {
  const MobileModalOverlay({
    required this.background,
    required this.child,
    super.key,
  });

  final Widget background;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        background,
        ColoredBox(color: context.colors.background.neutralScrim),
        Align(
          alignment: Alignment.bottomCenter,
          child: MobileModalCard(child: child),
        ),
      ],
    );
  }
}

class _ModalInnerHighlightPainter extends CustomPainter {
  const _ModalInnerHighlightPainter(this.shape);

  final ShapeBorder shape;

  @override
  void paint(Canvas canvas, Size size) {
    final path = shape.getOuterPath(Offset.zero & size);
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = const Color(0x26FFFFFF)
      ..maskFilter = const ui.MaskFilter.blur(ui.BlurStyle.inner, 2);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_ModalInnerHighlightPainter oldDelegate) =>
      oldDelegate.shape != shape;
}

const List<BoxShadow> _modalShadow = [
  BoxShadow(color: Color(0x14000000), offset: Offset(0, 14), blurRadius: 28),
  BoxShadow(color: Color(0x08000000), offset: Offset(0, -6), blurRadius: 12),
  BoxShadow(color: Color(0x0F000000), offset: Offset(0, 2), blurRadius: 8),
];

class MobileModalScaffold extends StatelessWidget {
  const MobileModalScaffold({
    required this.title,
    required this.onClose,
    required this.child,
    this.leading,
    this.titleStyle,
    this.titleMaxLines = 1,
    this.showTitle = true,
    this.showClose = true,
    this.bodyGap = AppSpacing.sm,
    this.bottomPadding = AppSpacing.md,
    this.constrainBody = false,
    super.key,
  });

  final String title;
  final VoidCallback onClose;
  final Widget child;

  final bool constrainBody;
  final Widget? leading;
  final TextStyle? titleStyle;
  final int titleMaxLines;
  final bool showTitle;
  final bool showClose;

  final double bodyGap;

  final double bottomPadding;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Stack(
      children: [
        Padding(
          padding: EdgeInsets.only(
            top: AppSpacing.base,
            bottom: bottomPadding,
            left: AppSpacing.sm,
            right: AppSpacing.sm,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (showTitle) ...[
                ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 26),
                  child: Padding(
                    padding: const EdgeInsets.only(right: 40),
                    child: Row(
                      children: [
                        if (leading != null) ...[
                          leading!,
                          const SizedBox(width: AppSpacing.s),
                        ],
                        Expanded(
                          child: Text(
                            title,
                            maxLines: titleMaxLines,
                            overflow: TextOverflow.ellipsis,
                            style:
                                titleStyle ??
                                AppTypography.bodyLarge.copyWith(
                                  fontWeight: FontWeight.w600,
                                  color: colors.text.accent,
                                ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                SizedBox(height: bodyGap),
              ],
              Flexible(
                child: constrainBody
                    ? child
                    : SingleChildScrollView(child: child),
              ),
            ],
          ),
        ),
        if (showClose)
          Positioned(
            top: 15.5,
            right: AppSpacing.sm,
            child: _ModalCloseButton(onTap: onClose),
          ),
      ],
    );
  }
}

class _ModalCloseButton extends StatefulWidget {
  const _ModalCloseButton({required this.onTap});

  final VoidCallback onTap;

  @override
  State<_ModalCloseButton> createState() => _ModalCloseButtonState();
}

class _ModalCloseButtonState extends State<_ModalCloseButton> {
  var _hovered = false;
  var _pressed = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final secondary = colors.button.secondary;
    final fill = _pressed
        ? secondary.bgPressed
        : _hovered
        ? secondary.bgHover
        : secondary.bg;
    return Semantics(
      label: 'Close',
      button: true,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onTap,
          onTapDown: (_) => setState(() => _pressed = true),
          onTapUp: (_) => setState(() => _pressed = false),
          onTapCancel: () => setState(() => _pressed = false),
          child: Container(
            width: 32,
            height: 32,
            padding: const EdgeInsets.all(AppSpacing.xxs),
            decoration: BoxDecoration(color: fill, shape: BoxShape.circle),
            child: Center(
              child: AppIcon(
                AppIcons.cross,
                size: 20,
                color: colors.icon.accent,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

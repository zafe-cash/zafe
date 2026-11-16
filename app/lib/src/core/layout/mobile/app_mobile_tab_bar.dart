import 'dart:async';
import 'dart:ui';

import 'package:flutter/widgets.dart';

import '../../feedback/app_haptics.dart';
import '../../theme/app_theme.dart';
import '../../widgets/app_icon.dart';

const double kMobileTabBarHeight = 64;

class AppMobileTabItem {
  const AppMobileTabItem({
    required this.iconName,
    required this.label,
    this.badge = false,
  });

  final String iconName;

  final String label;

  final bool badge;
}

class AppMobileTabBar extends StatelessWidget {
  const AppMobileTabBar({
    required this.items,
    required this.currentIndex,
    required this.onSelect,
    super.key,
  });

  final List<AppMobileTabItem> items;
  final int currentIndex;
  final ValueChanged<int> onSelect;

  static const _blur = 17.5;
  static const _itemHeight = 56.0;
  static const _iconSize = 28.0;

  static const selectDuration = Duration(milliseconds: 240);
  static const _selectCurve = Cubic(0.34, 1.40, 0.64, 1.0);

  @visibleForTesting
  static const activePillKey = ValueKey('mobile_tab_bar_active_pill');

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final radius = BorderRadius.circular(AppRadii.full);
    final motion = !(MediaQuery.maybeDisableAnimationsOf(context) ?? false);
    final pillAlignment = Alignment(
      items.length > 1 ? -1 + 2 * currentIndex / (items.length - 1) : 0,
      0,
    );

    return SizedBox(
      height: kMobileTabBarHeight,
      child: DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: radius,
          boxShadow: [
            BoxShadow(
              color: colors.macosUtility.thinBorder,
              blurRadius: 0,
              spreadRadius: 1,
            ),
            BoxShadow(
              color: const Color(0xFF000000).withValues(alpha: 0.05),
              offset: const Offset(0, 25),
              blurRadius: 25,
            ),
          ],
        ),
        child: ClipRRect(
          borderRadius: radius,
          child: BackdropFilter(
            filter: ImageFilter.blur(sigmaX: _blur, sigmaY: _blur),
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: colors.macosUtility.navPanel,
                borderRadius: radius,
              ),
              child: Padding(
                padding: const EdgeInsets.all(AppSpacing.xxs),
                child: Stack(
                  children: [
                    AnimatedAlign(
                      key: activePillKey,
                      duration: motion ? selectDuration : Duration.zero,
                      curve: _selectCurve,
                      alignment: pillAlignment,
                      child: FractionallySizedBox(
                        widthFactor: 1 / items.length,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: colors.navPanel.activeBg,
                            borderRadius: BorderRadius.circular(AppRadii.full),
                            border: Border.all(
                              color: colors.border.subtleOpacity,
                            ),
                          ),
                          child: const SizedBox(height: _itemHeight),
                        ),
                      ),
                    ),
                    Row(
                      children: [
                        for (var i = 0; i < items.length; i++)
                          Expanded(
                            child: _TabBarItem(
                              item: items[i],
                              active: i == currentIndex,
                              motion: motion,
                              onTap: () => onSelect(i),
                            ),
                          ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _TabBarItem extends StatefulWidget {
  const _TabBarItem({
    required this.item,
    required this.active,
    required this.motion,
    required this.onTap,
  });

  final AppMobileTabItem item;
  final bool active;
  final bool motion;
  final VoidCallback onTap;

  @override
  State<_TabBarItem> createState() => _TabBarItemState();
}

class _TabBarItemState extends State<_TabBarItem>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pop = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 250),
  );
  late final Animation<double> _popScale = TweenSequence<double>([
    TweenSequenceItem(
      tween: Tween(
        begin: 1.0,
        end: 1.12,
      ).chain(CurveTween(curve: Curves.easeOut)),
      weight: 45,
    ),
    TweenSequenceItem(
      tween: Tween(
        begin: 1.12,
        end: 1.0,
      ).chain(CurveTween(curve: Curves.easeIn)),
      weight: 55,
    ),
  ]).animate(_pop);

  var _pressed = false;

  @override
  void didUpdateWidget(covariant _TabBarItem oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!oldWidget.active && widget.active && widget.motion) {
      _pop.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _pop.dispose();
    super.dispose();
  }

  void _setPressed(bool pressed) {
    if (!widget.motion || _pressed == pressed) return;
    setState(() => _pressed = pressed);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final iconColor = widget.active
        ? colors.navPanel.activeIcon
        : colors.icon.muted;

    return Semantics(
      label: widget.item.badge
          ? '${widget.item.label}, needs you'
          : widget.item.label,
      button: true,
      selected: widget.active,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (_) => _setPressed(true),
        onTapCancel: () => _setPressed(false),
        onTapUp: (_) => _setPressed(false),
        onTap: () {
          unawaited(AppHaptics.auxiliaryKey());
          widget.onTap();
        },
        child: SizedBox(
          height: AppMobileTabBar._itemHeight,
          child: Center(
            child: AnimatedScale(
              scale: _pressed ? 0.9 : 1.0,
              duration: Duration(milliseconds: _pressed ? 90 : 180),
              curve: _pressed ? Curves.easeOut : Curves.easeOutBack,
              child: ScaleTransition(
                scale: _popScale,
                child: TweenAnimationBuilder<Color?>(
                  tween: ColorTween(end: iconColor),
                  duration: widget.motion
                      ? AppMobileTabBar.selectDuration
                      : Duration.zero,
                  builder: (_, color, _) => Stack(
                    clipBehavior: Clip.none,
                    children: [
                      AppIcon(
                        widget.item.iconName,
                        size: AppMobileTabBar._iconSize,
                        color: color ?? iconColor,
                        active: widget.active,
                      ),
                      Positioned(
                        right: -3,
                        top: -2,
                        child: AnimatedScale(
                          scale: widget.item.badge ? 1 : 0,
                          duration: const Duration(milliseconds: 200),
                          curve: Curves.easeOutBack,
                          child: Container(
                            width: 10,
                            height: 10,
                            decoration: BoxDecoration(
                              color: context.colors.background.brandStrong,
                              shape: BoxShape.circle,
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

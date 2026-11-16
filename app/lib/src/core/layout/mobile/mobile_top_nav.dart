import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';
import '../../widgets/app_icon.dart';

const double kMobileTopNavHeight = 72;

class MobileTopNav extends StatelessWidget {
  const MobileTopNav.account({
    required this.accountName,
    this.balanceLabel,
    this.syncLabel,
    this.syncLabelColor,
    this.syncIndicatorColor,
    this.syncAnimated = false,
    this.syncHighlightColor,
    this.avatar,
    this.onAccountTap,
    this.onSyncTap,
    super.key,
  }) : _variant = _MobileTopNavVariant.account,
       title = '',
       titleStyle = null,
       titleMaxLines = 1,
       height = kMobileTopNavHeight,
       progress = 0,
       showBackButton = true,
       onBack = null,
       trailing = null,
       foregroundColor = null,
       backIcon = AppIcons.chevronBackward;

  const MobileTopNav.steps({
    required this.progress,
    this.onBack,
    this.showBackButton = true,
    super.key,
  }) : _variant = _MobileTopNavVariant.steps,
       accountName = '',
       balanceLabel = null,
       syncLabel = null,
       syncLabelColor = null,
       syncIndicatorColor = null,
       syncAnimated = false,
       syncHighlightColor = null,
       avatar = null,
       onAccountTap = null,
       onSyncTap = null,
       title = '',
       titleStyle = null,
       titleMaxLines = 1,
       height = kMobileTopNavHeight,
       trailing = null,
       foregroundColor = null,
       backIcon = AppIcons.chevronBackward;

  const MobileTopNav.back({
    required this.title,
    this.onBack,
    this.trailing,
    this.backIcon = AppIcons.chevronBackward,
    this.titleStyle,
    this.titleMaxLines = 1,
    this.foregroundColor,
    this.height = kMobileTopNavHeight,
    super.key,
  }) : _variant = _MobileTopNavVariant.back,
       accountName = '',
       balanceLabel = null,
       syncLabel = null,
       syncLabelColor = null,
       syncIndicatorColor = null,
       syncAnimated = false,
       syncHighlightColor = null,
       avatar = null,
       onAccountTap = null,
       onSyncTap = null,
       progress = 0,
       showBackButton = true;

  final _MobileTopNavVariant _variant;

  final String accountName;
  final String? balanceLabel;
  final String? syncLabel;
  final Color? syncLabelColor;
  final Color? syncIndicatorColor;

  final bool syncAnimated;

  final Color? syncHighlightColor;

  final Widget? avatar;
  final VoidCallback? onAccountTap;

  final VoidCallback? onSyncTap;

  final double progress;
  final bool showBackButton;

  final String title;
  final TextStyle? titleStyle;
  final int titleMaxLines;
  final Color? foregroundColor;
  final double height;

  final Widget? trailing;

  final VoidCallback? onBack;

  final String backIcon;

  static const _avatarSize = 40.0;
  static const _backButtonSize = 44.0;
  static const _progressTrackWidth = 196.0;
  static const _progressTrackHeight = 6.0;
  static const _syncIndicatorSize = Size(6, 50);

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: height,
      child: switch (_variant) {
        _MobileTopNavVariant.account => _buildAccount(context),
        _MobileTopNavVariant.steps => _buildSteps(context),
        _MobileTopNavVariant.back => _buildBack(context),
      },
    );
  }

  Widget _buildAccount(BuildContext context) {
    final colors = context.colors;
    final account = GestureDetector(
      key: const ValueKey('mobile_top_nav_account'),
      behavior: HitTestBehavior.opaque,
      onTap: onAccountTap,
      child: Row(
        mainAxisSize: MainAxisSize.max,
        children: [
          avatar ?? _AvatarPlaceholder(size: _avatarSize),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  accountName,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppTypography.labelLarge.copyWith(
                    fontWeight: FontWeight.w600,
                    color: colors.text.accent,
                  ),
                ),
                if (balanceLabel != null) ...[
                  const SizedBox(height: AppSpacing.xxs),
                  Text(
                    balanceLabel!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppTypography.labelLarge.copyWith(
                      fontWeight: FontWeight.w400,
                      color: colors.text.secondary,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );

    return LayoutBuilder(
      builder: (context, constraints) => Row(
        children: [
          const SizedBox(width: AppSpacing.sm),
          Expanded(
            child: Align(alignment: Alignment.centerLeft, child: account),
          ),
          if (syncLabel != null)
            ConstrainedBox(
              constraints: BoxConstraints(maxWidth: constraints.maxWidth * 0.6),
              child: _tappableSync(
                _SyncStatus(
                  label: syncLabel!,
                  baseColor: syncLabelColor ?? colors.sync.text,
                  highlightColor:
                      syncHighlightColor ?? syncLabelColor ?? colors.sync.text,
                  indicatorColor: syncIndicatorColor ?? colors.sync.glow,
                  indicatorSize: _syncIndicatorSize,
                  animated: syncAnimated,
                ),
              ),
            )
          else
            const SizedBox(width: AppSpacing.sm),
        ],
      ),
    );
  }

  Widget _tappableSync(Widget status) {
    final onTap = onSyncTap;
    if (onTap == null) return status;
    return Semantics(
      button: true,
      onTap: onTap,
      child: GestureDetector(
        key: const ValueKey('mobile_top_nav_sync'),
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: status,
      ),
    );
  }

  Widget _buildSteps(BuildContext context) {
    final colors = context.colors;
    return Stack(
      alignment: Alignment.center,
      children: [
        Center(
          child: SizedBox(
            width: _progressTrackWidth,
            height: _progressTrackHeight,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: colors.background.overlay,
                borderRadius: BorderRadius.circular(AppRadii.full),
              ),
              child: Align(
                alignment: Alignment.centerLeft,
                child: FractionallySizedBox(
                  widthFactor: progress.clamp(0.0, 1.0),
                  heightFactor: 1,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: colors.background.inverse,
                      borderRadius: BorderRadius.circular(AppRadii.full),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        if (showBackButton)
          Positioned(
            left: AppSpacing.s,
            child: _BackButton(size: _backButtonSize, onTap: onBack),
          ),
      ],
    );
  }

  Widget _buildBack(BuildContext context) {
    final colors = context.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: AppSpacing.sm),
      child: Row(
        children: [
          if (onBack != null) ...[
            _BackButton(
              size: 40,
              onTap: onBack,
              iconName: backIcon,
              color: foregroundColor,
              boxed: true,
            ),
            const SizedBox(width: AppSpacing.s),
          ],
          Expanded(
            child: Text(
              title,
              maxLines: titleMaxLines,
              overflow: TextOverflow.ellipsis,
              style: (titleStyle ?? _zafeTitleStyle).copyWith(
                color: foregroundColor ?? colors.text.accent,
              ),
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}

const _zafeTitleStyle = AppTypography.heading;

enum _MobileTopNavVariant { account, steps, back }

class _BackButton extends StatelessWidget {
  const _BackButton({
    required this.size,
    this.onTap,
    this.iconName = AppIcons.chevronBackward,
    this.color,
    this.boxed = false,
  });

  final double size;
  final VoidCallback? onTap;
  final String iconName;
  final Color? color;
  final bool boxed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      label: iconName == AppIcons.cross ? 'Close' : 'Back',
      button: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          width: size,
          height: size,
          decoration: boxed
              ? BoxDecoration(
                  color: context.colors.background.base,
                  borderRadius: BorderRadius.circular(AppRadii.small),
                  border: Border.all(color: context.colors.border.subtle),
                )
              : null,
          child: Center(
            child: AppIcon(
              iconName,
              size: boxed ? 20 : 24,
              color: color ?? context.colors.icon.accent,
            ),
          ),
        ),
      ),
    );
  }
}

class _AvatarPlaceholder extends StatelessWidget {
  const _AvatarPlaceholder({required this.size});

  final double size;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: context.colors.background.overlay,
        shape: BoxShape.circle,
      ),
    );
  }
}

abstract final class _SyncStatusMotion {
  static const period = Duration(milliseconds: 1400);

  static const frameInterval = Duration(milliseconds: 60);

  static const _bandHalf = 0.18;

  static const _minGlowBlur = 8.0;
  static const _maxGlowBlur = 13.0;
  static const _minGlowAlpha = 0.2;
  static const _maxGlowAlpha = 0.45;

  static double _breath(double t) => (1 - math.cos(2 * math.pi * t)) / 2;

  static ({double blur, double alpha}) glowFor(double t) {
    final e = _breath(t);
    return (
      blur: _lerp(_minGlowBlur, _maxGlowBlur, e),
      alpha: _lerp(_minGlowAlpha, _maxGlowAlpha, e),
    );
  }

  static double _lerp(double a, double b, double t) => a + (b - a) * t;
}

class _SyncStatus extends StatefulWidget {
  const _SyncStatus({
    required this.label,
    required this.baseColor,
    required this.highlightColor,
    required this.indicatorColor,
    required this.indicatorSize,
    required this.animated,
  });

  final String label;
  final Color baseColor;
  final Color highlightColor;
  final Color indicatorColor;
  final Size indicatorSize;
  final bool animated;

  @override
  State<_SyncStatus> createState() => _SyncStatusState();
}

class _SyncStatusState extends State<_SyncStatus> {
  final _progress = ValueNotifier<double>(0);
  final _clock = Stopwatch();
  Timer? _ticker;

  bool get _shouldAnimate {
    if (!widget.animated) {
      return false;
    }
    return !(MediaQuery.maybeOf(context)?.disableAnimations ?? false);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncAnimation();
  }

  @override
  void didUpdateWidget(covariant _SyncStatus oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.animated != widget.animated) {
      _syncAnimation();
    }
  }

  void _syncAnimation() {
    if (_shouldAnimate) {
      if (_ticker == null) {
        _clock
          ..reset()
          ..start();
        _ticker = Timer.periodic(_SyncStatusMotion.frameInterval, (_) {
          _progress.value =
              (_clock.elapsedMilliseconds %
                  _SyncStatusMotion.period.inMilliseconds) /
              _SyncStatusMotion.period.inMilliseconds;
        });
      }
    } else {
      _ticker?.cancel();
      _ticker = null;
      _clock.stop();
      _progress.value = 0;
    }
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _progress.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RepaintBoundary(child: _content(context));
  }

  Widget _content(BuildContext context) {
    if (!_shouldAnimate) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Flexible(
            child: Text(
              widget.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: AppTypography.labelMedium.copyWith(
                color: widget.baseColor,
              ),
            ),
          ),
          const SizedBox(width: AppSpacing.s),
          _SyncEdgeIndicator(
            size: widget.indicatorSize,
            color: widget.indicatorColor,
          ),
        ],
      );
    }

    return AnimatedBuilder(
      animation: _progress,
      builder: (context, _) {
        final t = _progress.value;
        final glow = _SyncStatusMotion.glowFor(t);
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Flexible(
              child: _ShimmerLabel(
                label: widget.label,
                baseColor: widget.baseColor,
                highlightColor: widget.highlightColor,
                progress: t,
              ),
            ),
            const SizedBox(width: AppSpacing.s),
            _SyncEdgeIndicator(
              size: widget.indicatorSize,
              color: widget.indicatorColor,
              glowBlur: glow.blur,
              glowAlpha: glow.alpha,
            ),
          ],
        );
      },
    );
  }
}

class _ShimmerLabel extends StatelessWidget {
  const _ShimmerLabel({
    required this.label,
    required this.baseColor,
    required this.highlightColor,
    required this.progress,
  });

  final String label;
  final Color baseColor;
  final Color highlightColor;
  final double progress;

  @override
  Widget build(BuildContext context) {
    return ShaderMask(
      blendMode: BlendMode.srcIn,
      shaderCallback: (bounds) {
        final shift = (progress * 2 - 1) * bounds.width;
        final rect = Rect.fromLTWH(
          bounds.left + shift,
          bounds.top,
          bounds.width,
          bounds.height,
        );
        return LinearGradient(
          begin: Alignment.centerLeft,
          end: Alignment.centerRight,
          colors: [baseColor, highlightColor, baseColor],
          stops: const [
            0.5 - _SyncStatusMotion._bandHalf,
            0.5,
            0.5 + _SyncStatusMotion._bandHalf,
          ],
          tileMode: TileMode.clamp,
        ).createShader(rect);
      },
      child: Text(
        label,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: AppTypography.labelMedium.copyWith(
          color: const Color(0xFFFFFFFF),
        ),
      ),
    );
  }
}

class _SyncEdgeIndicator extends StatelessWidget {
  const _SyncEdgeIndicator({
    required this.size,
    required this.color,
    this.glowBlur = 12,
    this.glowAlpha = 0.6,
  });

  final Size size;
  final Color color;
  final double glowBlur;
  final double glowAlpha;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size.width,
      height: size.height,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.horizontal(
          left: Radius.circular(size.width / 2),
        ),
        boxShadow: [
          BoxShadow(
            color: color.withValues(alpha: glowAlpha),
            blurRadius: glowBlur,
          ),
        ],
      ),
    );
  }
}

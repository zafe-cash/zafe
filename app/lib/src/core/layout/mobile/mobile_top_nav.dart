import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../theme/app_theme.dart';
import '../../widgets/app_icon.dart';

/// Height of [MobileTopNav] — Figma `Mobile Top Nav` (node 4237:92733).
const double kMobileTopNavHeight = 72;

/// Mobile top navigation bar with the three Figma variants:
///
/// - [MobileTopNav.account] — avatar + account name (+ optional balance)
///   on the left, sync status label + edge glow indicator on the right.
///   Used by the tab roots.
/// - [MobileTopNav.steps] — back button + centered progress track. Used
///   by multi-step flows (onboarding, send wizard).
/// - [MobileTopNav.back] — back button + centered serif title. Used by
///   pushed detail screens; with a null [onBack] the button is omitted
///   and only the centered title remains (tab roots like Activity).
///
/// Metrics come from Figma node 4237:92733 (393×72; not tokenized as
/// variables yet, so they live here as constants). The Back-variant
/// title uses [AppTypography.headlineLarge], which on mobile resolves to
/// Space Grotesk 32 px (Headline L) — the size the shared Mobile Top Nav
/// component's Back variant uses across the mobile screen frames.
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

  /// Account variant: display name, optional secondary balance line, and
  /// the right-aligned sync status label. The label and edge-indicator
  /// colors default to the synced tokens; pass overrides for syncing /
  /// failed states.
  final String accountName;
  final String? balanceLabel;
  final String? syncLabel;
  final Color? syncLabelColor;
  final Color? syncIndicatorColor;

  /// When true (the live syncing state, reduced-motion off) the label
  /// shimmers a bright band across itself and the edge bar breathes
  /// a slow glow pulse. Otherwise the label and bar render static.
  final bool syncAnimated;

  /// Bright peak used for the shimmer band and the breathing glow.
  /// Falls back to [syncLabelColor] when null.
  final Color? syncHighlightColor;

  /// Account variant: leading 40×40 avatar. Falls back to a plain
  /// surface circle until real profile pictures are wired in.
  final Widget? avatar;
  final VoidCallback? onAccountTap;

  /// Account variant: tapping the sync status (e.g. to explain a failure).
  final VoidCallback? onSyncTap;

  /// Steps variant: progress through the flow, 0.0–1.0.
  final double progress;
  final bool showBackButton;

  /// Back variant: centered serif title.
  final String title;
  final TextStyle? titleStyle;
  final int titleMaxLines;
  final Color? foregroundColor;
  final double height;

  /// Back variant: right-aligned widget (e.g. the swap composer's
  /// "Powered by NEAR Intents" lockup — Figma 4686:102067).
  final Widget? trailing;

  final VoidCallback? onBack;

  /// Back-variant leading icon. Defaults to the chevron; the swap composer
  /// swaps it to a cross while its number-pad keyboard is open so the leading
  /// button dismisses the keyboard instead of leaving the tab.
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

    // Zafe: the status keeps its natural width at the right edge, but never more than
    // 60% of the bar (its label ellipsizes), so the vault name and avatar keep room on
    // narrow phones with large system text.
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

  // Zafe: a boxed back button with the title left-aligned beside it.
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

/// Subtle/slow motion constants for the syncing affordance. One full
/// breath (glow swell) and one shimmer sweep per [period]; both are
/// driven by the same controller so the label and bar feel coordinated.
abstract final class _SyncStatusMotion {
  static const period = Duration(milliseconds: 1400);

  /// How often the sweep and the glow move. Not every vsync: any change redraws the whole
  /// screen (about 5 ms of raster time per frame on a phone, measured with the profiler:
  /// the cost is the redraw itself, whatever the widget), and a display runs at 60-120 Hz,
  /// so a per-vsync animation held a core at ~70% for as long as a sync lasted. A 1.4 s
  /// sweep stays smooth at ~16 updates a second and costs about a seventh of that.
  static const frameInterval = Duration(milliseconds: 60);

  /// Half-width of the shimmer highlight band as a gradient-stop fraction.
  /// Kept narrow for a gentle, low-contrast sparkle.
  static const _bandHalf = 0.18;

  /// Edge-bar glow breathing range (shadow blur radius + alpha). Kept gentle
  /// so the syncing glow stays subtle rather than vibrant.
  static const _minGlowBlur = 8.0;
  static const _maxGlowBlur = 13.0;
  static const _minGlowAlpha = 0.2;
  static const _maxGlowAlpha = 0.45;

  /// 0 → 1 → 0 once per [period].
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

/// The right-aligned sync status: the label plus the edge glow bar.
///
/// While [animated] (the live syncing state, reduced-motion off) a timer steps a bright
/// shimmer band sweeping across
/// the label and a slow breathing pulse on the edge bar, so the two
/// read as one "actively working" affordance. Otherwise — synced, failed,
/// or reduced-motion — it renders the same static label + bar as before.
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
  /// 0..1 through one [_SyncStatusMotion.period], stepped by [_ticker].
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
    // Re-evaluate on dependency changes (e.g. the reduce-motion setting).
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
    // Its own layer: the motion repaints only this corner, never the screen under it.
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

/// The sync label with a bright highlight band sweeping across it.
///
/// A [ShaderMask] (`srcIn`) replaces the glyph pixels with a horizontal
/// `base → highlight → base` gradient; sliding the gradient's mapping
/// rect by [progress] travels the band left→right. The band fully exits
/// both edges (pure base) at the loop ends, so the repeat is seamless.
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
      // Solid color so `srcIn` keeps the gradient over the full glyph.
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

/// The thin glow bar hugging the right screen edge in the account
/// variant — Figma `_Nav Sync Widget > light` (node 4237:92744).
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

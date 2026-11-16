import 'dart:math' as math;

import 'package:flutter/material.dart' show Scaffold;
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import '../../layout/mobile/mobile_top_nav.dart' show kMobileTopNavHeight;
import '../../layout/app_column.dart';
import '../../theme/app_theme.dart';
import '../app_button.dart';
import '../app_icon.dart';

enum MobileTransactionProgressPhase { inProgress, pending, succeeded, failed }

const _sendingCircleColor = Color(0xFF00736C);
const _successCircleColor = Color(0xFFAE7C00);
const _failureCircleColor = Color(0xFFA82571);
const _statusIconColor = Color(0xFFFFFFFF);
const _statusCircleSize = 64.0;
const _statusIconSize = 32.0;
const _statusButtonWidth = 230.0;

class MobileTransactionProgressScreen extends StatelessWidget {
  const MobileTransactionProgressScreen({
    required this.phase,
    required this.title,
    required this.body,
    required this.canPop,
    this.onPopBlocked,
    this.primaryActionLabel,
    this.onPrimaryAction,
    this.secondaryActionLabel,
    this.onSecondaryAction,
    this.bodyMaxWidth = 223,
    this.background,
    this.successEffect,
    this.titleKey,
    this.bodyKey,
    this.statusBadgeKey,
    this.progressIconKey,
    this.successIconKey,
    this.failureIconKey,
    this.successRippleKey,
    this.primaryActionKey,
    this.secondaryActionKey,
    super.key,
  });

  final MobileTransactionProgressPhase phase;
  final String title;
  final String body;
  final bool canPop;
  final VoidCallback? onPopBlocked;
  final String? primaryActionLabel;
  final VoidCallback? onPrimaryAction;
  final String? secondaryActionLabel;
  final VoidCallback? onSecondaryAction;
  final double? bodyMaxWidth;

  final Widget? background;

  final Widget? successEffect;
  final Key? titleKey;
  final Key? bodyKey;
  final Key? statusBadgeKey;
  final Key? progressIconKey;
  final Key? successIconKey;
  final Key? failureIconKey;
  final Key? successRippleKey;
  final Key? primaryActionKey;
  final Key? secondaryActionKey;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final titleColor = phase == MobileTransactionProgressPhase.failed
        ? colors.text.destructive
        : colors.text.accent;
    final showPrimary = primaryActionLabel != null && onPrimaryAction != null;
    final showSecondary =
        secondaryActionLabel != null && onSecondaryAction != null;

    return PopScope<void>(
      canPop: canPop,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) onPopBlocked?.call();
      },
      child: Scaffold(
        backgroundColor: colors.background.window,
        body: Stack(
          fit: StackFit.expand,
          children: [
            if (background != null)
              Positioned.fill(
                child:
                    narrowerThanWindow(
                      context,
                      MediaQuery.sizeOf(context).width,
                    )
                    ? SideFade(child: background!)
                    : background!,
              ),
            SafeArea(
              child: Column(
                children: [
                  const SizedBox(height: kMobileTopNavHeight),
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, box) => SingleChildScrollView(
                        clipBehavior: Clip.none,
                        child: ConstrainedBox(
                          constraints: BoxConstraints(minHeight: box.maxHeight),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: AppSpacing.sm,
                              vertical: AppSpacing.s,
                            ),
                            child: Center(
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  MobileTransactionProgressBadge(
                                    phase: phase,
                                    badgeKey: statusBadgeKey,
                                    progressIconKey: progressIconKey,
                                    successIconKey: successIconKey,
                                    failureIconKey: failureIconKey,
                                    successRippleKey: successRippleKey,
                                    successEffect: successEffect,
                                  ),
                                  const SizedBox(height: AppSpacing.xl),
                                  Padding(
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: AppSpacing.md,
                                    ),
                                    child: _KeepTallest(
                                      child: AnimatedSwitcher(
                                        layoutBuilder: (current, previous) =>
                                            Stack(
                                              alignment: Alignment.topCenter,
                                              children: [...previous, ?current],
                                            ),
                                        duration: const Duration(
                                          milliseconds: 420,
                                        ),
                                        transitionBuilder: (child, animation) {
                                          final a = CurvedAnimation(
                                            parent: animation,
                                            curve: const Interval(
                                              0.5,
                                              1,
                                              curve: Curves.easeOutCubic,
                                            ),
                                          );
                                          return FadeTransition(
                                            opacity: a,
                                            child: SlideTransition(
                                              position: Tween(
                                                begin: const Offset(0, 0.12),
                                                end: Offset.zero,
                                              ).animate(a),
                                              child: child,
                                            ),
                                          );
                                        },
                                        child: Column(
                                          key: ValueKey(phase),
                                          children: [
                                            Text(
                                              title,
                                              key: titleKey,
                                              textAlign: TextAlign.center,
                                              style: AppTypography.displayLarge
                                                  .copyWith(color: titleColor),
                                            ),
                                            const SizedBox(
                                              height: AppSpacing.s,
                                            ),
                                            LayoutBuilder(
                                              builder: (context, constraints) {
                                                final requestedWidth =
                                                    bodyMaxWidth;
                                                final maxWidth =
                                                    requestedWidth == null
                                                    ? constraints.maxWidth
                                                    : math.min(
                                                        requestedWidth,
                                                        constraints.maxWidth,
                                                      );
                                                return ConstrainedBox(
                                                  constraints: BoxConstraints(
                                                    maxWidth: maxWidth,
                                                  ),
                                                  child: Text(
                                                    body,
                                                    key: bodyKey,
                                                    textAlign: TextAlign.center,
                                                    style: AppTypography
                                                        .bodyMediumStrong
                                                        .copyWith(
                                                          color: colors
                                                              .text
                                                              .primary,
                                                        ),
                                                  ),
                                                );
                                              },
                                            ),
                                          ],
                                        ),
                                      ),
                                    ),
                                  ),
                                  const SizedBox(height: AppSpacing.xl),
                                  SizedBox(
                                    width: _statusButtonWidth,
                                    child: showPrimary || showSecondary
                                        ? Column(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.stretch,
                                            children: [
                                              if (showPrimary)
                                                AppButton(
                                                  key: primaryActionKey,
                                                  onPressed: onPrimaryAction,
                                                  expand: true,
                                                  constrainContent: true,
                                                  child: Text(
                                                    primaryActionLabel!,
                                                  ),
                                                ),
                                              if (showPrimary && showSecondary)
                                                const SizedBox(
                                                  height: AppSpacing.s,
                                                ),
                                              if (showSecondary)
                                                AppButton(
                                                  key: secondaryActionKey,
                                                  onPressed: onSecondaryAction,
                                                  expand: true,
                                                  constrainContent: true,
                                                  variant:
                                                      AppButtonVariant.ghost,
                                                  child: Text(
                                                    secondaryActionLabel!,
                                                  ),
                                                ),
                                            ],
                                          )
                                        : const SizedBox(
                                            height: AppButtonSizing.largeHeight,
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
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class MobileTransactionProgressBadge extends StatefulWidget {
  const MobileTransactionProgressBadge({
    required this.phase,
    this.badgeKey,
    this.progressIconKey,
    this.successIconKey,
    this.failureIconKey,
    this.successRippleKey,
    this.inProgressCircleColor,
    this.inProgressIconColor,
    this.terminalAnimationEnabled = true,
    this.successEffect,
    super.key,
  });

  final MobileTransactionProgressPhase phase;

  final Widget? successEffect;
  final Key? badgeKey;
  final Key? progressIconKey;
  final Key? successIconKey;
  final Key? failureIconKey;
  final Key? successRippleKey;
  final Color? inProgressCircleColor;
  final Color? inProgressIconColor;

  final bool terminalAnimationEnabled;

  @override
  State<MobileTransactionProgressBadge> createState() =>
      _MobileTransactionProgressBadgeState();
}

class _MobileTransactionProgressBadgeState
    extends State<MobileTransactionProgressBadge>
    with TickerProviderStateMixin {
  late final AnimationController _ripple = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 650),
  );
  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 500),
  );

  late final AnimationController _pop = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 620),
  );

  late final Animation<double> _squeeze = TweenSequence<double>([
    TweenSequenceItem(
      tween: Tween(
        begin: 1.0,
        end: 0.86,
      ).chain(CurveTween(curve: Curves.easeOut)),
      weight: 13,
    ),
    TweenSequenceItem(tween: ConstantTween(0.86), weight: 87),
  ]).animate(_pop);

  late final Animation<double> _grow = TweenSequence<double>([
    TweenSequenceItem(tween: ConstantTween(0.0), weight: 13),
    TweenSequenceItem(
      tween: Tween(
        begin: 0.0,
        end: 1.14,
      ).chain(CurveTween(curve: Curves.easeOutCubic)),
      weight: 30,
    ),
    TweenSequenceItem(
      tween: Tween(
        begin: 1.14,
        end: 1.0,
      ).chain(CurveTween(curve: Curves.easeInOutSine)),
      weight: 57,
    ),
  ]).animate(_pop);
  late final Animation<double> _checkDraw = CurveTween(
    curve: const Interval(0.14, 0.62, curve: Curves.easeOutCubic),
  ).animate(_pop);

  static const _shakeAmplitude = 20.0;

  @override
  void initState() {
    super.initState();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncTerminalAnimation();
  }

  @override
  void didUpdateWidget(covariant MobileTransactionProgressBadge oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.terminalAnimationEnabled) {
      _resetTerminalAnimations();
      return;
    }
    if (oldWidget.phase != widget.phase ||
        !oldWidget.terminalAnimationEnabled) {
      _startTerminalAnimation(widget.phase);
    }
  }

  void _startTerminalAnimation(MobileTransactionProgressPhase phase) {
    if (!widget.terminalAnimationEnabled ||
        (MediaQuery.maybeOf(context)?.disableAnimations ?? false)) {
      _resetTerminalAnimations();
      return;
    }
    switch (phase) {
      case MobileTransactionProgressPhase.succeeded:
        _ripple.forward(from: 0);
        _pop.forward(from: 0);
      case MobileTransactionProgressPhase.failed:
        _shake.forward(from: 0);
      case MobileTransactionProgressPhase.inProgress:
      case MobileTransactionProgressPhase.pending:
        break;
    }
  }

  void _syncTerminalAnimation() {
    if (!widget.terminalAnimationEnabled) {
      _resetTerminalAnimations();
      return;
    }
    final disabled = MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    if (disabled) {
      _startTerminalAnimation(widget.phase);
      return;
    }
    switch (widget.phase) {
      case MobileTransactionProgressPhase.succeeded:
        if (_ripple.status == AnimationStatus.dismissed) {
          _ripple.forward();
          _pop.forward();
        }
      case MobileTransactionProgressPhase.failed:
        if (_shake.status == AnimationStatus.dismissed) {
          _shake.forward();
        }
      case MobileTransactionProgressPhase.inProgress:
      case MobileTransactionProgressPhase.pending:
        break;
    }
  }

  void _resetTerminalAnimations() {
    _ripple
      ..stop()
      ..value = 0;
    _shake
      ..stop()
      ..value = 0;
    _pop
      ..stop()
      ..value = 1;
  }

  Widget _successCircle(Widget check) {
    Widget disc(Color color) => Container(
      width: _statusCircleSize,
      height: _statusCircleSize,
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    );
    return Stack(
      alignment: Alignment.center,
      children: [
        AnimatedBuilder(
          animation: _pop,
          builder: (context, child) => Opacity(
            opacity: _pop.value < 0.45 ? 1 : 0,
            child: Transform.scale(scale: _squeeze.value, child: child),
          ),
          child: Stack(
            alignment: Alignment.center,
            children: [
              disc(widget.inProgressCircleColor ?? _sendingCircleColor),
              AppIcon(
                AppIcons.loader,
                size: _statusIconSize,
                color: widget.inProgressIconColor ?? _statusIconColor,
              ),
            ],
          ),
        ),
        ScaleTransition(scale: _grow, child: disc(_successCircleColor)),
        check,
      ],
    );
  }

  @override
  void dispose() {
    _ripple.dispose();
    _shake.dispose();
    _pop.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final animationsDisabled =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final circleColor = switch (widget.phase) {
      MobileTransactionProgressPhase.inProgress ||
      MobileTransactionProgressPhase.pending =>
        widget.inProgressCircleColor ?? _sendingCircleColor,
      MobileTransactionProgressPhase.succeeded => _successCircleColor,
      MobileTransactionProgressPhase.failed => _failureCircleColor,
    };
    final icon = switch (widget.phase) {
      MobileTransactionProgressPhase.inProgress ||
      MobileTransactionProgressPhase.pending => AppIcon(
        AppIcons.loader,
        key:
            widget.progressIconKey ??
            const ValueKey('mobile_transaction_progress_icon_loader'),
        size: _statusIconSize,
        color: widget.inProgressIconColor ?? _statusIconColor,
      ),
      MobileTransactionProgressPhase.succeeded => AnimatedBuilder(
        key:
            widget.successIconKey ??
            const ValueKey('mobile_transaction_progress_icon_success'),
        animation: _checkDraw,
        builder: (context, child) =>
            ClipRect(clipper: _RevealClipper(_checkDraw.value), child: child),
        child: AppIcon(
          AppIcons.checkCircle,
          size: _statusIconSize,
          color: _statusIconColor,
          patina: _statusIconColor,
        ),
      ),
      MobileTransactionProgressPhase.failed => AppIcon(
        AppIcons.warning,
        key:
            widget.failureIconKey ??
            const ValueKey('mobile_transaction_progress_icon_failed'),
        size: _statusIconSize,
        color: _statusIconColor,
        patina: _statusIconColor,
      ),
    };

    return SizedBox(
      key: widget.badgeKey,
      width: _statusCircleSize,
      height: _statusCircleSize,
      child: Stack(
        clipBehavior: Clip.none,
        alignment: Alignment.center,
        children: [
          if (widget.phase == MobileTransactionProgressPhase.succeeded &&
              widget.successEffect != null &&
              !animationsDisabled)
            OverflowBox(
              maxWidth: double.infinity,
              maxHeight: double.infinity,
              child: widget.successEffect,
            ),
          AnimatedBuilder(
            animation: _ripple,
            builder: (context, _) {
              final t = ((_ripple.value - 0.25) / 0.75).clamp(0.0, 1.0);
              if (widget.phase != MobileTransactionProgressPhase.succeeded ||
                  t <= 0 ||
                  t >= 1) {
                return const SizedBox.shrink();
              }
              return IgnorePointer(
                child: CustomPaint(
                  key:
                      widget.successRippleKey ??
                      const ValueKey(
                        'mobile_transaction_progress_success_ripple',
                      ),
                  painter: _WavePainter(t),
                ),
              );
            },
          ),
          AnimatedBuilder(
            animation: _shake,
            builder: (context, child) {
              final t = _shake.value;
              final dx = t <= 0 || t >= 1
                  ? 0.0
                  : math.sin(t * math.pi * 5) * _shakeAmplitude * (1 - t);
              return Transform.translate(
                key: const ValueKey(
                  'mobile_transaction_progress_failure_shake',
                ),
                offset: Offset(dx, 0),
                child: child,
              );
            },
            child:
                widget.phase == MobileTransactionProgressPhase.succeeded &&
                    !animationsDisabled
                ? _successCircle(icon)
                : AnimatedContainer(
                    duration: animationsDisabled
                        ? Duration.zero
                        : const Duration(milliseconds: 250),
                    width: _statusCircleSize,
                    height: _statusCircleSize,
                    decoration: BoxDecoration(
                      color: circleColor,
                      shape: BoxShape.circle,
                    ),
                    child: Center(
                      child: animationsDisabled
                          ? icon
                          : AnimatedSwitcher(
                              duration: const Duration(milliseconds: 250),
                              child: icon,
                            ),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _KeepTallest extends SingleChildRenderObjectWidget {
  const _KeepTallest({super.child});

  @override
  RenderObject createRenderObject(BuildContext context) => _RenderKeepTallest();
}

class _RenderKeepTallest extends RenderProxyBox {
  double _tallest = 0;

  @override
  void performLayout() {
    final child = this.child!;
    child.layout(constraints.loosen(), parentUsesSize: true);
    _tallest = math.max(_tallest, child.size.height);
    size = constraints.constrain(Size(child.size.width, _tallest));
  }
}

class _RevealClipper extends CustomClipper<Rect> {
  const _RevealClipper(this.fraction);
  final double fraction;

  @override
  Rect getClip(Size size) =>
      Rect.fromLTWH(0, 0, size.width * fraction.clamp(0.0, 1.0), size.height);

  @override
  bool shouldReclip(_RevealClipper old) => old.fraction != fraction;
}

class _WavePainter extends CustomPainter {
  _WavePainter(this.t);
  final double t;

  @override
  void paint(Canvas canvas, Size size) {
    final e = Curves.easeOutCubic.transform(t);
    canvas.drawCircle(
      size.center(Offset.zero),
      _statusCircleSize / 2 + 120 * e,
      Paint()
        ..color = _successCircleColor.withValues(alpha: 0.7 * (1 - t))
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1 + 4 * (1 - e),
    );
  }

  @override
  bool shouldRepaint(_WavePainter old) => old.t != t;
}

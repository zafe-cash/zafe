import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/layout/app_column.dart';
import '../../core/theme/app_theme.dart';

String _asset(BuildContext context, String name) {
  final dark = identical(context.colors, AppColors.dark);
  return 'assets/illustrations/${name}_${dark ? 'dark' : 'light'}.svg';
}

const kHeroArtKey = ValueKey('hero-art');

class OnboardingHero extends StatelessWidget {
  const OnboardingHero(this.name, {super.key, this.art});

  final String name;

  final Widget? art;

  static Size sizeIn(Size window) {
    final height = math.min(window.width * 1240 / 1080, window.height * 0.66);
    return Size(height * 1080 / 1240, height);
  }

  @override
  Widget build(BuildContext context) {
    final window = context.colors.background.window;
    final screen = MediaQuery.sizeOf(context);
    final size = sizeIn(screen);
    Widget fade(LinearGradient gradient) => Positioned.fill(
      child: IgnorePointer(
        child: DecoratedBox(decoration: BoxDecoration(gradient: gradient)),
      ),
    );
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: ExcludeSemantics(
        child: Align(
          alignment: Alignment.topCenter,
          child: SizedBox(
            key: kHeroArtKey,
            width: size.width,
            height: size.height,
            child: _maybeSideFade(
              narrowerThanWindow(context, size.width),
              Stack(
                children: [
                  Positioned.fill(
                    child:
                        art ??
                        SvgPicture.asset(
                          _asset(context, name),
                          fit: BoxFit.cover,
                        ),
                  ),
                  fade(
                    LinearGradient(
                      begin: Alignment.topCenter,
                      end: Alignment.bottomCenter,
                      stops: const [0, 0.1, 0.4, 0.74],
                      colors: [
                        window.withValues(alpha: 0.85),
                        window.withValues(alpha: 0),
                        window.withValues(alpha: 0),
                        window,
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

Widget _maybeSideFade(bool fade, Widget child) =>
    fade ? SideFade(child: child) : child;

class OnboardingBanner extends StatelessWidget {
  const OnboardingBanner(this.name, {super.key, this.aspectRatio = 1080 / 560});

  final String name;
  final double aspectRatio;

  @override
  Widget build(BuildContext context) {
    return ExcludeSemantics(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(AppRadii.large),
        child: AspectRatio(
          aspectRatio: aspectRatio,
          child: SvgPicture.asset(_asset(context, name), fit: BoxFit.cover),
        ),
      ),
    );
  }
}

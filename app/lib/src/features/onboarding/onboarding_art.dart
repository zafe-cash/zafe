import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/layout/app_column.dart';
import '../../core/theme/app_theme.dart';

/// Original Zafe illustrations in `assets/illustrations`, one SVG per
/// theme (`<name>_dark.svg` / `<name>_light.svg`).
String _asset(BuildContext context, String name) {
  final dark = identical(context.colors, AppColors.dark);
  return 'assets/illustrations/${name}_${dark ? 'dark' : 'light'}.svg';
}

/// The box the hero art is drawn in (tests check nothing to tap sits on it).
const kHeroArtKey = ValueKey('hero-art');

/// Art pinned to the top of the screen that fades into the window colour, so text and
/// buttons below sit on a clean background (content sits at the bottom). Full width on a
/// phone; in a wide or short window (landscape, a tablet, a flip phone's cover screen) at
/// most two thirds of the window's height, centred, its sides fading out too.
class OnboardingHero extends StatelessWidget {
  const OnboardingHero(this.name, {super.key, this.art});

  final String name;

  /// Drawn instead of the still scene (an animated version of it, same canvas).
  final Widget? art;

  /// The art's size in a window of [window] (the scene is 1080 x 1240).
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
      // Only paint: an animated scene below may take taps.
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
                  // Soft scrim under the status bar, clear art, then a fade into the
                  // window colour before the text starts.
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

/// Rounded banner at the top of a form screen (1080 x 560 art unless
/// [aspectRatio] says otherwise, e.g. the smaller empty-state banners).
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

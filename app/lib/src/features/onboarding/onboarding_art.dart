import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/theme/app_theme.dart';

/// Original Zafe illustrations in `assets/illustrations`, one SVG per
/// theme (`<name>_dark.svg` / `<name>_light.svg`).
String _asset(BuildContext context, String name) {
  final dark = identical(context.colors, AppColors.dark);
  return 'assets/illustrations/${name}_${dark ? 'dark' : 'light'}.svg';
}

/// Full-width art pinned to the top of the screen that fades into the window
/// colour, so text and buttons below sit on a clean background
/// (content sits at the bottom).
class OnboardingHero extends StatelessWidget {
  const OnboardingHero(this.name, {super.key, this.art});

  final String name;

  /// Drawn instead of the still scene (an animated version of it, same canvas).
  final Widget? art;

  @override
  Widget build(BuildContext context) {
    final window = context.colors.background.window;
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: ExcludeSemantics(
        child: Stack(
          children: [
            AspectRatio(
              aspectRatio: 1080 / 1240,
              child:
                  art ??
                  SvgPicture.asset(_asset(context, name), fit: BoxFit.cover),
            ),
            Positioned.fill(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    // Soft scrim under the status bar, clear art, then a
                    // fade into the window colour before the text starts.
                    stops: const [0, 0.1, 0.4, 0.74],
                    colors: [
                      window.withValues(alpha: 0.85),
                      window.withValues(alpha: 0),
                      window.withValues(alpha: 0),
                      window,
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

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

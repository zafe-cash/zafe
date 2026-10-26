import 'dart:io' show Platform;

import 'app_typography.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import 'app_radii.dart';
import 'colors/app_colors.dart';

/// Material theme for the Material widgets Zafe still uses (dialogs, pickers,
/// switches, text selection, scaffolds). Its [ColorScheme] is built from the
/// app's own tokens ([AppColors], "Verdigris", docs/brand.md), so Material
/// widgets match the rest of the app. Screens style themselves through
/// `context.colors`; don't read `Theme.of(context)` colours in new code.
ColorScheme _scheme(AppColors c, Brightness brightness) => ColorScheme(
  brightness: brightness,
  // Surfaces
  surface: c.background.ground,
  onSurface: c.text.accent,
  surfaceContainerLowest: c.background.window,
  surfaceContainerLow: c.background.base,
  surfaceContainer: c.background.raised,
  surfaceContainerHigh: c.background.raised,
  surfaceContainerHighest: c.background.overlay,
  onSurfaceVariant: c.text.secondary,
  // Brand: primary buttons, focus, selection
  primary: c.button.primary.bg,
  onPrimary: c.button.primary.label,
  primaryContainer: c.background.brandSubtle,
  onPrimaryContainer: c.text.brand,
  // Value (Zcash gold)
  secondary: c.text.value,
  onSecondary: c.text.inverse,
  secondaryContainer: c.background.valueAlpha,
  onSecondaryContainer: c.text.value,
  tertiary: c.text.warning,
  onTertiary: c.text.inverse,
  // Error (rose)
  error: c.text.destructive,
  onError: c.button.destructive.label,
  errorContainer: c.background.destructiveSubtle,
  onErrorContainer: c.text.destructive,
  // Lines
  outline: c.border.regular,
  outlineVariant: c.border.subtle,
  inverseSurface: c.background.inverse,
  onInverseSurface: c.text.inverse,
  inversePrimary: c.text.brand,
  shadow: const Color(0xFF000000),
  scrim: const Color(0xFF000000),
);

// ---------------------------------------------------------------------------
// Text themes
// ---------------------------------------------------------------------------

const _bodyFamily = 'DM Sans';

/// Material's text styles, for widgets that pick one by themselves (dialogs, menus,
/// selectable text): each is a role from the app's scale (AppTypography), so nothing
/// renders off it or under 14.
TextTheme _buildTextTheme(Color textColor) {
  TextStyle c(TextStyle s) => s.copyWith(color: textColor);
  return TextTheme(
    displayLarge: c(AppTypography.hero),
    displayMedium: c(AppTypography.title),
    displaySmall: c(AppTypography.title),
    headlineLarge: c(AppTypography.title),
    headlineMedium: c(AppTypography.title),
    headlineSmall: c(AppTypography.heading),
    titleLarge: c(AppTypography.heading),
    titleMedium: c(AppTypography.labelLarge),
    titleSmall: c(AppTypography.labelMedium),
    bodyLarge: c(AppTypography.bodyMedium),
    bodyMedium: c(AppTypography.labelMedium),
    bodySmall: c(AppTypography.bodySmall),
    labelLarge: c(AppTypography.labelLarge),
    labelMedium: c(AppTypography.labelMedium),
    labelSmall: c(AppTypography.labelMedium),
  );
}

// ---------------------------------------------------------------------------
// ThemeData builder — single source of truth for both light and dark
// ---------------------------------------------------------------------------

// Desktop platforms get instant page transitions (no slide/fade/zoom).
// Individual routes can still override via CustomTransitionPage in GoRouter.
class _NoTransitionsBuilder extends PageTransitionsBuilder {
  const _NoTransitionsBuilder();

  @override
  Duration get transitionDuration => Duration.zero;

  @override
  Duration get reverseTransitionDuration => Duration.zero;

  @override
  Widget buildTransitions<T>(
    PageRoute<T> route,
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) => child;
}

bool get _isDesktop =>
    Platform.isMacOS || Platform.isWindows || Platform.isLinux;

ThemeData _buildTheme(AppColors colors, Brightness brightness) {
  final colorScheme = _scheme(colors, brightness);
  return ThemeData(
    useMaterial3: true,
    dialogTheme: defaultTargetPlatform == TargetPlatform.iOS
        ? const DialogThemeData(
            shape: RoundedSuperellipseBorder(
              borderRadius: BorderRadius.all(Radius.circular(AppRadii.xLarge)),
            ),
          )
        : null,
    brightness: colorScheme.brightness,
    colorScheme: colorScheme,
    textTheme: _buildTextTheme(colorScheme.onSurface),
    // type: the default family for Material widgets without a style.
    fontFamily: _bodyFamily,
    scaffoldBackgroundColor: colors.background.window,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: colors.text.brand,
      selectionColor: colors.background.brandAlpha,
      selectionHandleColor: colors.text.brand,
    ),
    pageTransitionsTheme: _isDesktop
        ? const PageTransitionsTheme(
            builders: {
              TargetPlatform.macOS: _NoTransitionsBuilder(),
              TargetPlatform.windows: _NoTransitionsBuilder(),
              TargetPlatform.linux: _NoTransitionsBuilder(),
            },
          )
        : null,
    appBarTheme: AppBarTheme(
      backgroundColor: colorScheme.surface,
      foregroundColor: colorScheme.onSurface,
      elevation: 0,
      scrolledUnderElevation: 0,
      titleTextStyle: AppTypography.heading.copyWith(
        color: colorScheme.onSurface,
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: colorScheme.primary,
        foregroundColor: colorScheme.onPrimary,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        textStyle: AppTypography.labelLarge,
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: colorScheme.onSurface,
        side: BorderSide(color: colorScheme.outline),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        textStyle: AppTypography.labelLarge,
      ),
    ),
  );
}

ThemeData buildMaterialLightTheme() =>
    _buildTheme(AppColors.light, Brightness.light);
ThemeData buildMaterialDarkTheme() =>
    _buildTheme(AppColors.dark, Brightness.dark);

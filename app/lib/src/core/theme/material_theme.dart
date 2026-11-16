import 'dart:io' show Platform;

import 'app_typography.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import 'app_radii.dart';
import 'colors/app_colors.dart';

ColorScheme _scheme(AppColors c, Brightness brightness) => ColorScheme(
  brightness: brightness,
  surface: c.background.ground,
  onSurface: c.text.accent,
  surfaceContainerLowest: c.background.window,
  surfaceContainerLow: c.background.base,
  surfaceContainer: c.background.raised,
  surfaceContainerHigh: c.background.raised,
  surfaceContainerHighest: c.background.overlay,
  onSurfaceVariant: c.text.secondary,
  primary: c.button.primary.bg,
  onPrimary: c.button.primary.label,
  primaryContainer: c.background.brandSubtle,
  onPrimaryContainer: c.text.brand,
  secondary: c.text.value,
  onSecondary: c.text.inverse,
  secondaryContainer: c.background.valueAlpha,
  onSecondaryContainer: c.text.value,
  tertiary: c.text.warning,
  onTertiary: c.text.inverse,
  error: c.text.destructive,
  onError: c.button.destructive.label,
  errorContainer: c.background.destructiveSubtle,
  onErrorContainer: c.text.destructive,
  outline: c.border.regular,
  outlineVariant: c.border.subtle,
  inverseSurface: c.background.inverse,
  onInverseSurface: c.text.inverse,
  inversePrimary: c.text.brand,
  shadow: const Color(0xFF000000),
  scrim: const Color(0xFF000000),
);

const _bodyFamily = 'DM Sans';

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

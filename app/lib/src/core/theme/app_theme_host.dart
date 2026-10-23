import 'dart:async';
import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'app_theme.dart';

/// Resolves the user's [ThemeMode] into the app design-system theme and keeps
/// native platform chrome aligned with that resolved brightness.
class AppThemeHost extends StatelessWidget {
  const AppThemeHost({required this.themeMode, required this.child, super.key});

  final ThemeMode themeMode;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final brightness = _resolveBrightness(
      themeMode,
      MediaQuery.platformBrightnessOf(context),
    );
    final appThemeData = brightness == Brightness.dark
        ? AppThemeData.dark
        : AppThemeData.light;

    return AppTheme(
      data: appThemeData,
      child: _MacOSWindowAppearanceSync(
        brightness: brightness,
        child: _NativeAppearanceSync(
          themeMode: themeMode,
          brightness: brightness,
          child: _AndroidSystemBarsSync(brightness: brightness, child: child),
        ),
      ),
    );
  }

  static Brightness _resolveBrightness(
    ThemeMode themeMode,
    Brightness platformBrightness,
  ) {
    return switch (themeMode) {
      ThemeMode.system => platformBrightness,
      ThemeMode.dark => Brightness.dark,
      ThemeMode.light => Brightness.light,
    };
  }
}

class _MacOSWindowAppearanceSync extends StatefulWidget {
  const _MacOSWindowAppearanceSync({
    required this.brightness,
    required this.child,
  });

  final Brightness brightness;
  final Widget child;

  @override
  State<_MacOSWindowAppearanceSync> createState() =>
      _MacOSWindowAppearanceSyncState();
}

class _MacOSWindowAppearanceSyncState
    extends State<_MacOSWindowAppearanceSync> {
  @override
  void initState() {
    super.initState();
    _MacOSWindowAppearance.sync(widget.brightness);
  }

  @override
  void didUpdateWidget(covariant _MacOSWindowAppearanceSync oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.brightness == widget.brightness) return;
    _MacOSWindowAppearance.sync(widget.brightness);
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

class _NativeAppearanceSync extends StatefulWidget {
  const _NativeAppearanceSync({
    required this.themeMode,
    required this.brightness,
    required this.child,
  });

  final ThemeMode themeMode;
  final Brightness brightness;
  final Widget child;

  @override
  State<_NativeAppearanceSync> createState() => _NativeAppearanceSyncState();
}

class _NativeAppearanceSyncState extends State<_NativeAppearanceSync> {
  @override
  void initState() {
    super.initState();
    _NativeAppearance.sync(widget.themeMode, widget.brightness);
  }

  @override
  void didUpdateWidget(covariant _NativeAppearanceSync oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.themeMode == widget.themeMode &&
        oldWidget.brightness == widget.brightness) {
      return;
    }
    _NativeAppearance.sync(widget.themeMode, widget.brightness);
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// Keeps the Android system bars transparent with icons that contrast with
/// the app's theme. An [AnnotatedRegion] rather than a one-off
/// `SystemChrome` call: `MaterialApp` sends `SystemUiOverlayStyle.dark` or
/// `.light` on every build, whose black `systemNavigationBarColor` painted a
/// black band behind the navigation bar on Android 14 and older (Android 15+
/// ignores the colour). The region is read after each frame's build, so its
/// style is the one sent.
class _AndroidSystemBarsSync extends StatelessWidget {
  const _AndroidSystemBarsSync({required this.brightness, required this.child});

  final Brightness brightness;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.android) return child;
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: androidSystemBarsStyleFor(brightness),
      child: child,
    );
  }
}

/// Overlay style for the resolved app theme brightness. The bars are
/// transparent (`MainActivity` turns edge-to-edge on for every Android
/// version), so the app's own background shows behind them, and contrast
/// enforcement is off so the OS lays no scrim over them. The transparent
/// navigation colour overrides `MaterialApp`'s black one; Flutter applies it
/// below Android 15 only. `statusBarBrightness` is the iOS-side field and
/// stays unset; this style only ever applies on Android.
@visibleForTesting
SystemUiOverlayStyle androidSystemBarsStyleFor(Brightness brightness) {
  final icons = brightness == Brightness.dark
      ? Brightness.light
      : Brightness.dark;
  return SystemUiOverlayStyle(
    statusBarIconBrightness: icons,
    systemStatusBarContrastEnforced: false,
    systemNavigationBarColor: Colors.transparent,
    systemNavigationBarIconBrightness: icons,
    systemNavigationBarContrastEnforced: false,
  );
}

abstract final class _MacOSWindowAppearance {
  static const _channel = MethodChannel('xyz.zafe/window_appearance');

  static Brightness? _lastBrightness;

  static void sync(Brightness brightness) {
    if (kIsWeb || !Platform.isMacOS) return;
    if (_lastBrightness == brightness) return;
    _lastBrightness = brightness;
    unawaited(_setBrightness(brightness));
  }

  static Future<void> _setBrightness(Brightness brightness) async {
    try {
      await _channel.invokeMethod<void>('setBrightness', {
        'brightness': brightness == Brightness.dark ? 'dark' : 'light',
      });
    } catch (error) {
      _lastBrightness = null;
      debugPrint('MacOSWindowAppearance: sync failed: $error');
    }
  }
}

/// Hands the theme setting to the OS. On Android 12+ this sets the app's own
/// night mode, which the system splash screen uses on the next launch (it is
/// drawn before Flutter starts, so it can't read our prefs).
abstract final class _NativeAppearance {
  static const _channel = MethodChannel('xyz.zafe/window_appearance');

  static ThemeMode? _lastThemeMode;
  static Brightness? _lastResolvedBrightness;

  static void sync(ThemeMode themeMode, Brightness resolvedBrightness) {
    if (kIsWeb || !(Platform.isIOS || Platform.isAndroid)) return;
    if (_lastThemeMode == themeMode &&
        _lastResolvedBrightness == resolvedBrightness) {
      return;
    }
    _lastThemeMode = themeMode;
    _lastResolvedBrightness = resolvedBrightness;
    unawaited(_setBrightness(themeMode, resolvedBrightness));
  }

  static Future<void> _setBrightness(
    ThemeMode themeMode,
    Brightness resolvedBrightness,
  ) async {
    try {
      await _channel.invokeMethod<void>('setBrightness', {
        'brightness': switch (themeMode) {
          ThemeMode.system => 'system',
          ThemeMode.dark => 'dark',
          ThemeMode.light => 'light',
        },
      });
    } catch (error) {
      _lastThemeMode = null;
      _lastResolvedBrightness = null;
      debugPrint('NativeAppearance: sync failed: $error');
    }
  }
}

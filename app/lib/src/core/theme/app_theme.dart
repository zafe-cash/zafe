import 'package:flutter/widgets.dart';

import 'app_theme_data.dart';
import 'colors/app_colors.dart';

export 'app_icon_size.dart';
export 'app_radii.dart';
export 'app_serif_display.dart';
export 'app_shadows.dart';
export 'app_sizing.dart';
export 'app_spacing.dart';
export 'app_theme_data.dart';
export 'app_typography.dart';
export 'colors/app_colors.dart';

class AppTheme extends InheritedWidget {
  const AppTheme({super.key, required this.data, required super.child});

  final AppThemeData data;

  static AppThemeData of(BuildContext context) {
    final theme = context.dependOnInheritedWidgetOfExactType<AppTheme>();
    assert(
      theme != null,
      'AppTheme.of() was called without an AppTheme ancestor. '
      'Wrap MaterialApp.builder with AppTheme(data: ..., child: child!).',
    );
    return theme!.data;
  }

  @override
  bool updateShouldNotify(AppTheme oldWidget) => data != oldWidget.data;
}

extension AppThemeX on BuildContext {
  AppThemeData get appTheme => AppTheme.of(this);
  AppColors get colors => AppTheme.of(this).colors;
}

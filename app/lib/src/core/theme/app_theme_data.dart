import 'colors/app_colors.dart';

class AppThemeData {
  const AppThemeData({required this.colors});

  final AppColors colors;

  static const dark = AppThemeData(colors: AppColors.dark);
  static const light = AppThemeData(colors: AppColors.light);
}

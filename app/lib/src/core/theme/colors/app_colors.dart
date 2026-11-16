import 'app_background_colors.dart';
import 'app_border_colors.dart';
import 'app_button_colors.dart';
import 'app_fade_colors.dart';
import 'app_icon_colors.dart';
import 'app_macos_utility_colors.dart';
import 'app_nav_panel_colors.dart';
import 'app_shadow_colors.dart';
import 'app_state_colors.dart';
import 'app_surface_colors.dart';
import 'app_sync_colors.dart';
import 'app_text_colors.dart';
import 'app_vault_card_colors.dart';

export 'app_background_colors.dart';
export 'app_border_colors.dart';
export 'app_button_colors.dart';
export 'app_fade_colors.dart';
export 'app_icon_colors.dart';
export 'app_macos_utility_colors.dart';
export 'app_nav_panel_colors.dart';
export 'app_shadow_colors.dart';
export 'app_state_colors.dart';
export 'app_surface_colors.dart';
export 'app_sync_colors.dart';
export 'app_text_colors.dart';
export 'app_vault_card_colors.dart';

class AppColors {
  const AppColors({
    required this.background,
    required this.surface,
    required this.border,
    required this.text,
    required this.icon,
    required this.button,
    required this.state,
    required this.fade,
    required this.navPanel,
    required this.shadows,
    required this.sync,
    required this.macosUtility,
    required this.vaultCard,
  });

  final AppBackgroundColors background;
  final AppSurfaceColors surface;
  final AppBorderColors border;
  final AppTextColors text;
  final AppIconColors icon;
  final AppButtonColors button;
  final AppStateColors state;
  final AppFadeColors fade;
  final AppNavPanelColors navPanel;
  final AppShadowColors shadows;
  final AppSyncColors sync;
  final AppMacosUtilityColors macosUtility;
  final AppVaultCardColors vaultCard;

  static const dark = AppColors(
    background: AppBackgroundColors.dark,
    surface: AppSurfaceColors.dark,
    border: AppBorderColors.dark,
    text: AppTextColors.dark,
    icon: AppIconColors.dark,
    button: AppButtonColors.dark,
    state: AppStateColors.dark,
    fade: AppFadeColors.dark,
    navPanel: AppNavPanelColors.dark,
    shadows: AppShadowColors.dark,
    sync: AppSyncColors.dark,
    macosUtility: AppMacosUtilityColors.dark,
    vaultCard: AppVaultCardColors.dark,
  );

  static const light = AppColors(
    background: AppBackgroundColors.light,
    surface: AppSurfaceColors.light,
    border: AppBorderColors.light,
    text: AppTextColors.light,
    icon: AppIconColors.light,
    button: AppButtonColors.light,
    state: AppStateColors.light,
    fade: AppFadeColors.light,
    navPanel: AppNavPanelColors.light,
    shadows: AppShadowColors.light,
    sync: AppSyncColors.light,
    macosUtility: AppMacosUtilityColors.light,
    vaultCard: AppVaultCardColors.light,
  );
}

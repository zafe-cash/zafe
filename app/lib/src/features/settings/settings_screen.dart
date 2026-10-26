import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:permission_handler/permission_handler.dart'
    show openAppSettings;

import '../../core/platform/system_settings.dart';
import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_menu.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/security/app_lock.dart';
import '../../core/security/unlock_gate.dart';
import '../../providers/device_lock_provider.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/payment_sounds_provider.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/theme_mode_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/vault_relay.dart';
import '../../notifications/notification_primer.dart';
import '../backup/backup_prompt_screen.dart' show backupStatusProvider;
import '../vaults/vault_actions.dart' show openViewingKey;
import 'zcash_server_screen.dart' show endpointHost;
import 'tor_setting_row.dart';
import '../../core/widgets/app_segmented.dart';
import '../../core/widgets/app_switch.dart';
import '../../core/widgets/line_icon.dart';

const _rowHeight = 44.0;

/// The release's version, passed by the release workflows (`--dart-define=ZAFE_VERSION`).
const _appVersion = String.fromEnvironment(
  'ZAFE_VERSION',
  defaultValue: '0.1.0',
);

/// Settings: grouped surface cards of 44px rows.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final summary = vault.summary;
    final hideAmounts = ref.watch(privacyModeProvider);
    final paymentSounds = ref.watch(paymentSoundsProvider);
    final themeMode = ref.watch(themeModeProvider);
    final appLock = ref.watch(appLockDelayProvider);
    final hasScreenLock = ref.watch(hasScreenLockProvider).value;
    final endpoints = ref.watch(endpointsProvider);
    final vaultRelay = ref.watch(activeVaultRelayProvider);
    final moving = ref.watch(relayMoveProvider.select((m) => m.moving));
    final backedUp = ref.watch(backupStatusProvider).value;
    final notifications = ref.watch(notificationsEnabledProvider).value;
    if (summary == null) return const SizedBox.shrink();
    final vaultName = ref.watch(activeVaultNameProvider) ?? summary.name;

    final rowStyle = AppTypography.labelLarge.copyWith(
      fontWeight: FontWeight.w400,
      color: colors.text.accent,
    );
    MobileListRow row({
      required String icon,
      required String label,
      String? value,
      VoidCallback? onTap,
      bool chevron = false,
      Widget? trailing,
    }) => MobileListRow(
      leading: AppIcon(icon, size: 20, color: colors.icon.muted),
      label: label,
      value: value,
      trailing: trailing,
      minRowHeight: _rowHeight,
      textStyle: rowStyle,
      valueTextStyle: rowStyle,
      valueColor: colors.text.accent,
      chevronColor: colors.icon.accent,
      showChevron: chevron,
      onTap: onTap,
    );

    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            const MobileTopNav.back(title: 'Settings'),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(
                  AppSpacing.sm,
                  AppSpacing.s,
                  AppSpacing.sm,
                  // Clears the floating tab bar.
                  112,
                ),
                children: [
                  // What people come looking for first: privacy.
                  _Group(
                    title: 'Privacy',
                    rows: [
                      TorSettingRow(rowStyle: rowStyle),
                      row(
                        icon: hideAmounts ? AppIcons.eyeClosed : AppIcons.eye,
                        label: 'Hide amounts',
                        onTap: () =>
                            ref.read(privacyModeProvider.notifier).toggle(),
                        trailing: AppSwitch(
                          value: hideAmounts,
                          semanticsLabel: 'Hide amounts',
                          onChanged: (_) =>
                              ref.read(privacyModeProvider.notifier).toggle(),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  _Group(
                    title: 'Security',
                    rows: [
                      AppMenu(
                        items: [
                          for (final d in AppLockDelay.values)
                            AppMenuItem(
                              icon: d == AppLockDelay.off
                                  ? AppIcons.unlock
                                  : AppIcons.lock,
                              label: d.label,
                              selected: d == appLock,
                              onTap: () =>
                                  _setAppLock(context, ref, appLock, d),
                            ),
                        ],
                        builder: (context, open) => row(
                          icon: appLock == AppLockDelay.off
                              ? AppIcons.unlock
                              : AppIcons.lock,
                          label: 'Lock app',
                          value: appLock.label,
                          chevron: true,
                          onTap: open,
                        ),
                      ),
                      if (hasScreenLock == false) const _NoScreenLockNote(),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  // The open vault's own settings, by its name.
                  _Group(
                    title: vaultName,
                    rows: [
                      row(
                        icon: AppIcons.lock,
                        label: 'Back up',
                        value: switch (backedUp) {
                          true => 'Saved',
                          false => 'Not yet',
                          null => null,
                        },
                        chevron: true,
                        onTap: () => context.push('/export'),
                      ),
                      row(
                        icon: AppIcons.key,
                        label: 'Viewing key',
                        chevron: true,
                        onTap: () => openViewingKey(context, ref),
                      ),
                      row(
                        icon: AppIcons.shieldKeyhole,
                        label: 'How it\'s protected',
                        chevron: true,
                        onTap: () => context.push('/vault-protection'),
                      ),
                      row(
                        icon: AppIcons.server,
                        label: 'Server',
                        value: moving
                            ? 'Moving...'
                            : vaultRelay == null
                            ? null
                            : endpointHost(vaultRelay.url),
                        chevron: true,
                        onTap: () => context.push('/vault-server'),
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  _Group(
                    title: 'This phone',
                    rows: [
                      row(
                        icon: AppIcons.globe,
                        label: 'Zcash server',
                        value: endpointHost(endpoints.lightwalletdUrl),
                        chevron: true,
                        onTap: () => context.push('/zcash-server'),
                      ),
                      row(
                        icon: AppIcons.bell,
                        label: 'Notifications',
                        value: switch (notifications) {
                          true => 'On',
                          false => 'Off',
                          null => null,
                        },
                        chevron: true,
                        onTap: () async {
                          // Off: the system prompt (or the app's settings); on, only
                          // the system settings can turn them off.
                          if (notifications == true) {
                            await openAppSettings();
                          } else {
                            await enableNotifications(ref);
                          }
                          ref.invalidate(notificationsEnabledProvider);
                        },
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  _Group(
                    title: 'App',
                    rows: [
                      MobileListRow(
                        leading: AppIcon(
                          AppIcons.theme,
                          size: 20,
                          color: colors.icon.muted,
                        ),
                        label: 'Theme',
                        minRowHeight: _rowHeight,
                        textStyle: rowStyle,
                        trailing: AppSegmented<ThemeMode>(
                          value: themeMode,
                          options: const [
                            (ThemeMode.system, AppIcons.monitor, 'System'),
                            (ThemeMode.light, AppIcons.day, 'Light'),
                            (ThemeMode.dark, AppIcons.night, 'Dark'),
                          ],
                          onChanged: (m) =>
                              ref.read(themeModeProvider.notifier).set(m),
                        ),
                      ),
                      row(
                        icon: paymentSounds
                            ? AppIcons.sound
                            : AppIcons.soundOff,
                        label: 'Payment sounds',
                        onTap: () =>
                            ref.read(paymentSoundsProvider.notifier).toggle(),
                        trailing: AppSwitch(
                          value: paymentSounds,
                          semanticsLabel: 'Payment sounds',
                          onChanged: (_) =>
                              ref.read(paymentSoundsProvider.notifier).toggle(),
                        ),
                      ),
                      row(
                        icon: AppIcons.help,
                        label: 'Diagnostic report',
                        value: 'Stays on this phone',
                        chevron: true,
                        onTap: () => context.push('/diagnostics'),
                      ),
                      row(
                        icon: AppIcons.book,
                        label: 'Open-source licenses',
                        chevron: true,
                        onTap: () => showLicensePage(
                          context: context,
                          applicationName: 'Zafe',
                          applicationVersion: _appVersion,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  const SizedBox(height: AppSpacing.base),
                  Center(
                    child: Text(
                      'Zafe v$_appVersion',
                      style: AppTypography.codeSmall.copyWith(
                        color: colors.text.secondary,
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

  /// Turning the app lock off or making it wait longer needs an unlock; making it
  /// stricter doesn't.
  Future<void> _setAppLock(
    BuildContext context,
    WidgetRef ref,
    AppLockDelay current,
    AppLockDelay selected,
  ) async {
    if (selected == current) return;
    if (current.weakenedBy(selected) &&
        !await confirmUnlock(
          context,
          ref,
          reason: 'Unlock to change the app lock',
        )) {
      return;
    }
    await ref.read(appLockDelayProvider.notifier).set(selected);
  }
}

/// Shown under the Security rows when the phone has no screen lock, so approvals
/// go through without a prompt. "Open settings" goes to the phone's security settings;
/// coming back re-checks, so the note goes away once a lock is set.
class _NoScreenLockNote extends ConsumerStatefulWidget {
  const _NoScreenLockNote();

  @override
  ConsumerState<_NoScreenLockNote> createState() => _NoScreenLockNoteState();
}

class _NoScreenLockNoteState extends ConsumerState<_NoScreenLockNote> {
  late final AppLifecycleListener _lifecycle;

  @override
  void initState() {
    super.initState();
    _lifecycle = AppLifecycleListener(
      onResume: () => ref.invalidate(hasScreenLockProvider),
    );
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    _tap.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppSpacing.xxs,
        AppSpacing.xs,
        AppSpacing.xxs,
        AppSpacing.xxs,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LineIcon(
            size: 16,
            line: AppTypography.bodySmall,
            icon: AppIcon(
              AppIcons.warning,
              size: 16,
              color: colors.icon.warning,
              patina: colors.icon.warning,
            ),
          ),
          const SizedBox(width: AppSpacing.xs),
          Expanded(
            child: Text.rich(
              TextSpan(
                text:
                    'Set a screen lock on this phone to protect approvals. '
                    'Until then, Zafe can\'t ask for it. ',
                children: [
                  TextSpan(
                    text: 'Open settings',
                    style: TextStyle(
                      decoration: TextDecoration.underline,
                      decorationColor: colors.text.warning,
                    ),
                    recognizer: _tap,
                  ),
                ],
              ),
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.warning,
              ),
            ),
          ),
        ],
      ),
    );
  }

  late final _tap = TapGestureRecognizer()..onTap = openSecuritySettings;
}

class _Group extends StatelessWidget {
  const _Group({required this.title, required this.rows});
  final String title;
  final List<Widget> rows;

  @override
  Widget build(BuildContext context) {
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.fromLTRB(
        AppSpacing.sm,
        AppSpacing.base,
        AppSpacing.sm,
        AppSpacing.base,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(
              left: AppSpacing.xxs,
              bottom: AppSpacing.xs,
            ),
            child: Text(
              title,
              style: AppTypography.labelLarge.copyWith(
                fontWeight: FontWeight.w400,
                color: context.colors.text.secondary,
              ),
            ),
          ),
          ...rows,
        ],
      ),
    );
  }
}

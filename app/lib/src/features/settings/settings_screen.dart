import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/endpoints.dart';
import '../../core/platform/system_settings.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_copy_feedback.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/security/app_lock.dart';
import '../../core/security/unlock_gate.dart';
import '../../providers/device_lock_provider.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/payment_sounds_provider.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/theme_mode_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../home/rename_sheet.dart';
import 'endpoint_sheet.dart';
import 'diagnostics_sheet.dart';
import 'tor_sheet.dart';

const _rowHeight = 44.0;
const _appVersion = '0.1.0';

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
    final requireUnlock = ref.watch(requireUnlockProvider);
    final appLock = ref.watch(appLockDelayProvider);
    final hasScreenLock = ref.watch(hasScreenLockProvider).value;
    final me = vault.myKeyHex;
    final endpoints = ref.watch(endpointsProvider);
    final tor = ref.watch(torProvider);
    if (summary == null) return const SizedBox.shrink();

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
    }) => MobileListRow(
      leading: AppIcon(icon, size: 20, color: colors.icon.muted),
      label: label,
      value: value,
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
      body: AppToastHost(
        child: SafeArea(
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
                    _Group(
                      title: 'Vault',
                      rows: [
                        row(
                          icon: AppIcons.wallet,
                          label: 'Name',
                          value: ref.watch(activeVaultNameProvider),
                          chevron: true,
                          onTap: () => showRenameVaultSheet(context, ref),
                        ),
                        row(
                          icon: AppIcons.users,
                          label: 'Approval rule',
                          value:
                              '${summary.threshold} of ${summary.members.length} signers',
                        ),
                        row(
                          icon: AppIcons.qr,
                          label: 'Vault address',
                          value: compactAddress(summary.address),
                          chevron: true,
                          onTap: () => context.push('/receive'),
                        ),
                        row(
                          icon: AppIcons.globe,
                          label: 'Network',
                          value: _networkLabel(summary.network),
                        ),
                      ],
                    ),
                    const SizedBox(height: AppSpacing.md),
                    _Group(
                      title: 'Security',
                      rows: [
                        row(
                          icon: AppIcons.shieldKeyhole,
                          label: 'How it\'s protected',
                          chevron: true,
                          onTap: () => context.push('/vault-protection'),
                        ),
                        row(
                          icon: AppIcons.lock,
                          label: 'Back up vault',
                          chevron: true,
                          onTap: () => context.push('/export'),
                        ),
                        row(
                          icon: AppIcons.eye,
                          label: 'Viewing key',
                          chevron: true,
                          onTap: () => _openViewingKey(context, ref),
                        ),
                        row(
                          icon: AppIcons.key,
                          label: 'Your signer key',
                          value: me == null ? '' : memberLabel(me),
                          onTap: me == null
                              ? null
                              : () => copyTextWithToast(
                                  context,
                                  text: me,
                                  toastMessage: 'Signer key copied',
                                ),
                        ),
                        row(
                          icon: hideAmounts ? AppIcons.eyeClosed : AppIcons.eye,
                          label: 'Hide amounts',
                          value: hideAmounts ? 'On' : 'Off',
                          onTap: () =>
                              ref.read(privacyModeProvider.notifier).toggle(),
                        ),
                        row(
                          icon: paymentSounds
                              ? AppIcons.sound
                              : AppIcons.soundOff,
                          label: 'Payment sounds',
                          value: paymentSounds ? 'On' : 'Off',
                          onTap: () =>
                              ref.read(paymentSoundsProvider.notifier).toggle(),
                        ),
                        row(
                          icon: appLock == AppLockDelay.off
                              ? AppIcons.unlock
                              : AppIcons.lock,
                          label: 'Lock app',
                          value: appLock.label,
                          chevron: true,
                          onTap: () => _pickAppLock(context, ref, appLock),
                        ),
                        row(
                          icon: requireUnlock ? AppIcons.lock : AppIcons.unlock,
                          label: 'Require unlock to approve',
                          value: requireUnlock ? 'On' : 'Off',
                          onTap: () =>
                              _toggleRequireUnlock(context, ref, requireUnlock),
                        ),
                        if (hasScreenLock == false) const _NoScreenLockNote(),
                      ],
                    ),
                    const SizedBox(height: AppSpacing.md),
                    _Group(
                      title: 'Privacy',
                      rows: [
                        row(
                          icon: AppIcons.tor,
                          label: 'Use Tor',
                          value: tor.statusLabel,
                          chevron: true,
                          onTap: () => showTorSheet(context),
                        ),
                        row(
                          icon: AppIcons.help,
                          label: 'Diagnostic report',
                          value: 'Stays on this phone',
                          chevron: true,
                          onTap: () => showDiagnosticsSheet(context),
                        ),
                      ],
                    ),
                    const SizedBox(height: AppSpacing.md),
                    _Group(
                      title: 'System',
                      rows: [
                        row(
                          icon: AppIcons.theme,
                          label: 'Theme',
                          value: _themeLabel(themeMode),
                          chevron: true,
                          onTap: () => _pickTheme(context, ref, themeMode),
                        ),
                        row(
                          icon: AppIcons.endpoint,
                          label: 'Relay',
                          value: endpoints.relayIsPlaceholder
                              ? 'Not configured'
                              : endpointHost(endpoints.relayUrl),
                          chevron: true,
                          onTap: () => showEndpointSheet(
                            context,
                            ref,
                            EndpointKind.relay,
                          ),
                        ),
                        row(
                          icon: AppIcons.endpoint,
                          label: 'Zcash server',
                          value: endpointHost(endpoints.lightwalletdUrl),
                          chevron: true,
                          onTap: () => showEndpointSheet(
                            context,
                            ref,
                            EndpointKind.lightwalletd,
                          ),
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
                    AppButton(
                      expand: true,
                      variant: AppButtonVariant.destructive,
                      leading: const AppIcon(AppIcons.trash, size: 20),
                      onPressed: () => _confirmRemove(
                        context,
                        ref,
                        ref.read(activeVaultNameProvider) ?? summary.name,
                        summary.threshold,
                      ),
                      child: const Text('Remove vault from this device'),
                    ),
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
      ),
    );
  }

  Future<void> _confirmRemove(
    BuildContext context,
    WidgetRef ref,
    String name,
    int threshold,
  ) async {
    final remove = await showAppMobileSheet<bool>(
      context: context,
      builder: (sheet) {
        final colors = sheet.colors;
        return MobileModalScaffold(
          title: 'Remove "$name"?',
          onClose: () => Navigator.of(sheet).pop(false),
          leading: AppIcon(
            AppIcons.warning,
            size: 20,
            color: colors.icon.destructive,
            patina: colors.icon.destructive,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'This deletes your key share, signing state and history for this vault from '
                'this phone. You stay a member, but this phone can no longer approve or sign. '
                'If fewer than $threshold members keep their shares, the vault\'s funds can never '
                'be moved again.',
                style: AppTypography.bodyMedium.copyWith(
                  color: colors.text.accent,
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              AppButton(
                expand: true,
                variant: AppButtonVariant.destructive,
                onPressed: () => Navigator.of(sheet).pop(true),
                child: const Text('Remove from this device'),
              ),
              const SizedBox(height: AppSpacing.xs),
              AppButton(
                expand: true,
                variant: AppButtonVariant.ghost,
                onPressed: () => Navigator.of(sheet).pop(false),
                child: const Text('Keep it'),
              ),
            ],
          ),
        );
      },
    );
    if (remove != true || !context.mounted) return;
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to remove this vault',
    )) {
      return;
    }
    if (!context.mounted) return;
    final vaultId = ref.read(vaultProvider).activeId!;
    await ref.read(vaultProvider.notifier).removeVault(vaultId);
    if (!context.mounted) return;
    final next = ref.read(vaultProvider);
    context.go(
      next.hasVault
          ? '/home'
          : next.isSettingUp
          ? '/setup'
          : '/welcome',
    );
  }

  /// The viewing key reveals the vault's whole history, so it asks for an unlock first.
  Future<void> _openViewingKey(BuildContext context, WidgetRef ref) async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to show the viewing key',
    )) {
      return;
    }
    if (context.mounted) context.push('/viewing-key');
  }

  /// Turning the gate off needs an unlock (whatever the setting); turning it on
  /// doesn't.
  Future<void> _toggleRequireUnlock(
    BuildContext context,
    WidgetRef ref,
    bool current,
  ) async {
    if (current &&
        !await confirmUnlock(
          context,
          ref,
          reason: 'Unlock to stop asking before approvals',
          always: true,
        )) {
      return;
    }
    await ref.read(requireUnlockProvider.notifier).set(!current);
  }

  /// Turning the app lock off or making it wait longer needs an unlock; making it
  /// stricter doesn't.
  Future<void> _pickAppLock(
    BuildContext context,
    WidgetRef ref,
    AppLockDelay current,
  ) async {
    final selected = await showAppMobileSheet<AppLockDelay>(
      context: context,
      builder: (_) => _OptionsSheet<AppLockDelay>(
        title: 'Lock app',
        current: current,
        options: [
          for (final d in AppLockDelay.values)
            (
              d,
              d == AppLockDelay.off ? AppIcons.unlock : AppIcons.lock,
              d.label,
            ),
        ],
      ),
    );
    if (selected == null || selected == current || !context.mounted) return;
    if (current.weakenedBy(selected) &&
        !await confirmUnlock(
          context,
          ref,
          reason: 'Unlock to change the app lock',
          always: true,
        )) {
      return;
    }
    await ref.read(appLockDelayProvider.notifier).set(selected);
  }

  Future<void> _pickTheme(
    BuildContext context,
    WidgetRef ref,
    ThemeMode current,
  ) async {
    final selected = await showAppMobileSheet<ThemeMode>(
      context: context,
      builder: (_) => _OptionsSheet<ThemeMode>(
        title: 'Theme',
        current: current,
        options: const [
          (ThemeMode.system, AppIcons.monitor, 'System (Auto)'),
          (ThemeMode.light, AppIcons.day, 'Light'),
          (ThemeMode.dark, AppIcons.night, 'Dark'),
        ],
      ),
    );
    if (selected != null && selected != current) {
      await ref.read(themeModeProvider.notifier).set(selected);
    }
  }
}

String _networkLabel(String network) => switch (network) {
  'main' => 'Mainnet',
  'test' => 'Testnet',
  _ => 'Regtest',
};

String _themeLabel(ThemeMode mode) => switch (mode) {
  ThemeMode.system => 'System',
  ThemeMode.light => 'Light',
  ThemeMode.dark => 'Dark',
};

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
          AppIcon(
            AppIcons.warning,
            size: 16,
            color: colors.icon.warning,
            patina: colors.icon.warning,
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

/// Theme picker: option cards with a radio mark, committed with Update.
/// Pick one of a few options (theme, app lock); pops the chosen value on "Update".
class _OptionsSheet<T> extends StatefulWidget {
  const _OptionsSheet({
    required this.title,
    required this.options,
    required this.current,
  });
  final String title;
  final List<(T, String, String)> options;
  final T current;

  @override
  State<_OptionsSheet<T>> createState() => _OptionsSheetState<T>();
}

class _OptionsSheetState<T> extends State<_OptionsSheet<T>> {
  late T _selected = widget.current;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileModalScaffold(
      title: widget.title,
      onClose: () => Navigator.of(context).pop(),
      bodyGap: AppSpacing.md,
      bottomPadding: AppSpacing.base,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (mode, icon, label) in widget.options) ...[
            Semantics(
              button: true,
              selected: mode == _selected,
              label: label,
              excludeSemantics: true,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: () => setState(() => _selected = mode),
                child: Container(
                  height: 64,
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppSpacing.sm,
                  ),
                  decoration: BoxDecoration(
                    color: colors.background.ground,
                    borderRadius: BorderRadius.circular(AppRadii.medium),
                    border: Border.all(
                      color: mode == _selected
                          ? colors.border.strong
                          : colors.border.subtle,
                      width: mode == _selected ? 1.5 : 1,
                    ),
                  ),
                  child: Row(
                    children: [
                      Opacity(
                        opacity: mode == _selected ? 1 : 0.5,
                        child: AppIcon(
                          icon,
                          size: 20,
                          color: colors.icon.accent,
                        ),
                      ),
                      const SizedBox(width: AppSpacing.s),
                      Expanded(
                        child: Text(
                          label,
                          style: AppTypography.bodyMediumStrong.copyWith(
                            color: colors.text.accent,
                          ),
                        ),
                      ),
                      Container(
                        width: 24,
                        height: 24,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: mode == _selected
                              ? colors.background.inverse
                              : colors.background.raised,
                        ),
                        child: mode == _selected
                            ? Center(
                                child: AppIcon(
                                  AppIcons.check,
                                  size: 14,
                                  color: colors.text.inverse,
                                ),
                              )
                            : null,
                      ),
                    ],
                  ),
                ),
              ),
            ),
            const SizedBox(height: AppSpacing.xs),
          ],
          const SizedBox(height: AppSpacing.sm),
          AppButton(
            expand: true,
            onPressed: () => Navigator.of(context).pop(_selected),
            child: const Text('Update'),
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
        ],
      ),
    );
  }
}

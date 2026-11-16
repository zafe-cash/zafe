import 'core/platform/secure_screen.dart';
import 'dart:async';

import 'package:flutter/cupertino.dart' show CupertinoPage;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'core/layout/app_column.dart';
import 'core/security/app_lock_gate.dart';
import 'core/theme/app_theme_host.dart';
import 'core/theme/material_theme.dart';
import 'features/backup/backup_prompt_screen.dart';
import 'features/backup/export_screen.dart';
import 'features/backup/restore_screen.dart';
import 'features/home/home_screen.dart';
import 'features/home/vault_shell.dart';
import 'features/signers/signers_screen.dart';
import 'features/onboarding/create_vault_screen.dart';
import 'features/onboarding/join_vault_screen.dart';
import 'core/config/network_config.dart';
import 'features/scan/qr_scan_screen.dart';
import 'features/onboarding/setup_screen.dart';
import 'features/onboarding/welcome_screen.dart';
import 'features/proposals/activity_screen.dart';
import 'features/proposals/proposal_screen.dart';
import 'features/proposals/sending_screen.dart';
import 'features/receive/receive_screen.dart';
import 'features/relay_move/relay_move_screen.dart';
import 'features/received/received_screen.dart';
import 'features/send/payment_link.dart';
import 'features/send/payment_request_screen.dart';
import 'features/send/send_screen.dart';
import 'features/settings/relay_picker.dart';
import 'features/home/sync_status_screen.dart';
import 'features/settings/diagnostics_screen.dart';
import 'features/settings/settings_screen.dart';
import 'features/settings/vault_server_screen.dart';
import 'features/settings/zcash_server_screen.dart';
import 'features/recover/recover_screen.dart';
import 'features/settings/vault_protection_screen.dart';
import 'features/signers/replace_signer_screen.dart';
import 'features/settings/viewing_key_screen.dart';
import 'providers/device_lock_provider.dart';
import 'providers/tor_provider.dart';
import 'providers/mempool_watch_provider.dart';
import 'providers/proposals_provider.dart';
import 'providers/server_failover_provider.dart';
import 'providers/theme_mode_provider.dart';
import 'notifications/vault_updates.dart'
    show kKeepSeatPrefix, kReceivedPrefix, kSeatMovePrefix, kUnapprovedPrefix;
import 'notifications/vault_watch.dart';
import 'features/vaults/manage_vaults_screen.dart';
import 'providers/app_notices_provider.dart';
import 'providers/vault_provider.dart';
import 'services/app_update.dart';
import 'services/invite_links.dart';
import 'rust/api/relay_move.dart' as rust_move;

final _routerProvider = Provider<GoRouter>((ref) {
  final vault = ref.read(vaultProvider);
  final initial = vault.hasVault
      ? '/home'
      : vault.isSettingUp
      ? '/setup'
      : '/welcome';

  Page<void> page(Widget child) =>
      CupertinoPage(child: AppColumn(child: child));

  late final GoRouter router;
  Future<void> openTapped() async {
    final payload = notificationTaps.value;
    final ids = payload == null ? null : parsePayload(payload);
    if (ids == null) return;
    notificationTaps.value = null;
    final (vaultId, proposalId) = ids;
    final vaults = ref.read(vaultProvider);
    if (proposalId == kSetupPayload || proposalId == kVaultReadyPayload) {
      if (!vaults.vaults.any((v) => v.id == vaultId)) return;
      await ref.read(vaultProvider.notifier).showVault(vaultId);
      final settingUp = ref.read(vaultProvider).isSettingUp;
      router.go(
        settingUp
            ? '/setup'
            : proposalId == kVaultReadyPayload
            ? '/backup-prompt'
            : '/home',
      );
      return;
    }
    if (!vaults.vaults.any((v) => v.id == vaultId && v.ready)) return;
    if (vaults.activeId != vaultId || vaults.isAdding) {
      await ref.read(vaultProvider.notifier).switchTo(vaultId);
      router.go('/home');
    }
    if (proposalId.startsWith(kSeatMovePrefix)) {
      router.go('/signers');
      return;
    }
    if (proposalId.startsWith(kKeepSeatPrefix)) {
      router.go('/home');
      return;
    }
    if (proposalId.startsWith(kUnapprovedPrefix)) {
      router.go('/activity');
      return;
    }
    router.push(
      proposalId.startsWith(kReceivedPrefix)
          ? '/received/${proposalId.substring(kReceivedPrefix.length)}'
          : '/proposal/$proposalId',
    );
  }

  void onTap() => unawaited(openTapped());
  notificationTaps.addListener(onTap);
  ref.onDispose(() => notificationTaps.removeListener(onTap));
  WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(openTapped()));

  void openInviteLink() {
    final invite = inviteLinks.value;
    if (invite == null) return;
    if (ref.read(appLockedProvider)) return;
    final vaults = ref.read(vaultProvider);
    if (vaults.isSettingUp) return;
    if (ref.read(proposalsProvider).sends.values.any((s) => s.running)) return;
    final here = router.routerDelegate.currentConfiguration.uri.path;
    if (here == '/backup-prompt') return;
    inviteLinks.value = null;
    if (vaults.activeId != null) {
      ref.read(vaultProvider.notifier).beginAddVault();
    }
    router.go('/welcome');
    router.push(
      Uri(path: '/join', queryParameters: {'invite': invite}).toString(),
    );
  }

  void openMoveLink() {
    final link = moveLinks.value;
    if (link == null) return;
    if (ref.read(appLockedProvider)) return;
    final vaults = ref.read(vaultProvider);
    if (vaults.isSettingUp) return;
    if (ref.read(proposalsProvider).sends.values.any((s) => s.running)) return;
    final here = router.routerDelegate.currentConfiguration.uri.path;
    if (here == '/backup-prompt') return;
    moveLinks.value = null;
    String? vaultId;
    try {
      vaultId = rust_move.parseMoveLink(text: link).vaultId;
    } catch (_) {}
    final target = vaults.vaults.where((v) => v.id == vaultId && v.ready);
    if (target.isEmpty) {
      ref
          .read(appNoticesProvider.notifier)
          .show(
            const AppNotice(
              id: 'move-link',
              title: 'Move link',
              body: 'This link is for a vault that isn\'t on this phone.',
            ),
          );
      return;
    }
    unawaited(() async {
      await ref.read(vaultProvider.notifier).switchTo(vaultId!);
      router.push(
        Uri(
          path: '/vault-server/follow',
          queryParameters: {'link': link},
        ).toString(),
      );
    }());
  }

  moveLinks.addListener(openMoveLink);
  ref.onDispose(() => moveLinks.removeListener(openMoveLink));
  WidgetsBinding.instance.addPostFrameCallback((_) {
    void retry() => scheduleMicrotask(openMoveLink);
    router.routerDelegate.addListener(retry);
    ref.onDispose(() => router.routerDelegate.removeListener(retry));
    openMoveLink();
  });

  inviteLinks.addListener(openInviteLink);
  ref.onDispose(() => inviteLinks.removeListener(openInviteLink));
  WidgetsBinding.instance.addPostFrameCallback((_) {
    void retry() => scheduleMicrotask(openInviteLink);
    router.routerDelegate.addListener(retry);
    ref.onDispose(() => router.routerDelegate.removeListener(retry));
    openInviteLink();
  });

  void openPaymentLink() {
    final link = paymentLinks.value;
    if (link == null) return;
    void tell(String title, String body) => ref
        .read(appNoticesProvider.notifier)
        .show(AppNotice(id: 'payment-link', title: title, body: body));

    if (paymentLinkExpired(link, DateTime.now())) {
      paymentLinks.value = null;
      tell('Payment link expired', kPaymentLinkExpiredMessage);
      return;
    }
    if (ref.read(appLockedProvider)) return;
    final vaults = ref.read(vaultProvider);
    if (!vaults.vaults.any((v) => v.ready)) {
      paymentLinks.value = null;
      tell('No vault yet', kPaymentLinkNoVaultMessage);
      return;
    }
    if (vaults.isSettingUp && (vaults.membership?.sealed ?? false)) return;
    final here = router.routerDelegate.currentConfiguration.uri.path;
    if (here == '/backup-prompt') return;
    paymentLinks.value = null;
    here == '/payment-request'
        ? router.pushReplacement('/payment-request', extra: link)
        : router.push('/payment-request', extra: link);
  }

  paymentLinks.addListener(openPaymentLink);
  ref.onDispose(() => paymentLinks.removeListener(openPaymentLink));
  ref.listen(appLockedProvider, (_, _) {
    scheduleMicrotask(openPaymentLink);
    scheduleMicrotask(openInviteLink);
    scheduleMicrotask(openMoveLink);
  });
  ref.listen(serverFailoverProvider, (_, moved) {
    if (moved == null) return;
    ref
        .read(appNoticesProvider.notifier)
        .show(
          AppNotice(
            id: 'server-failover',
            title: 'Zcash server switched',
            body: moved.message,
          ),
        );
  });
  WidgetsBinding.instance.addPostFrameCallback((_) {
    void retry() => scheduleMicrotask(openPaymentLink);
    router.routerDelegate.addListener(retry);
    ref.onDispose(() => router.routerDelegate.removeListener(retry));
    openPaymentLink();
  });

  const serverRoutes = ['/pick-server', '/zcash-server'];
  return router = GoRouter(
    initialLocation: initial,
    overridePlatformDefaultLocation: true,
    redirect: (context, state) {
      final vault = ref.read(vaultProvider);
      final loc = state.matchedLocation;
      final inVault = const [
        '/home',
        '/receive',
        '/send',
        '/proposal',
        '/settings',
        '/signers',
        '/viewing-key',
        '/vault-protection',
        '/vaults',
        '/replace-signer',
        '/export',
        '/activity',
        '/backup-prompt',
        '/scan-recipient',
        '/scan-recovery',
        '/vault-server',
        '/status',
        '/diagnostics',
      ].any(loc.startsWith);
      if (loc == '/payment-request') return null;
      if (loc == '/scan-relay' || serverRoutes.any(loc.startsWith)) {
        return null;
      }
      if (vault.hasVault && !inVault) return '/home';
      if (!vault.hasVault && inVault) {
        return vault.isSettingUp ? '/setup' : '/welcome';
      }
      return null;
    },
    routes: [
      GoRoute(
        path: '/welcome',
        pageBuilder: (_, _) => const CupertinoPage(child: WelcomeScreen()),
      ),
      GoRoute(
        path: '/create',
        pageBuilder: (_, _) => page(const CreateVaultScreen()),
      ),
      GoRoute(
        path: '/join',
        pageBuilder: (_, state) => page(
          SecureScreen(
            child: JoinVaultScreen(
              initialInvite: state.uri.queryParameters['invite'],
            ),
          ),
        ),
      ),
      GoRoute(
        path: '/scan-invite',
        pageBuilder: (_, _) =>
            page(const SecureScreen(child: ScanInviteScreen())),
      ),
      GoRoute(
        path: '/scan-relay',
        pageBuilder: (_, _) => page(const ScanRelayScreen()),
      ),
      GoRoute(
        path: '/zcash-server',
        pageBuilder: (_, _) => page(const ZcashServerScreen()),
      ),
      GoRoute(
        path: '/pick-server',
        pageBuilder: (_, state) {
          final args = state.extra as RelayPickerArgs?;
          return page(
            args == null
                ? const RelayPickerScreen(title: 'Server', initialUrl: '')
                : RelayPickerScreen(
                    title: args.title,
                    initialUrl: args.selectedUrl,
                    currentUrl: args.currentUrl,
                    initialToken: args.token,
                    askToken: args.askToken,
                    confirmLabel: args.confirmLabel,
                  ),
          );
        },
      ),
      GoRoute(
        path: '/scan-seat-link',
        pageBuilder: (_, _) => page(const ScanSeatLinkScreen()),
      ),
      GoRoute(
        path: '/scan-recovery',
        pageBuilder: (_, _) =>
            page(const SecureScreen(child: ScanRecoveryScreen())),
      ),
      GoRoute(
        path: '/scan-recipient',
        pageBuilder: (_, _) =>
            page(const ScanRecipientScreen(network: kZafeNetwork)),
      ),
      GoRoute(
        path: '/setup',
        pageBuilder: (_, _) => page(const SecureScreen(child: SetupScreen())),
      ),
      StatefulShellRoute.indexedStack(
        builder: (_, _, shell) => AppColumn(child: VaultShell(shell: shell)),
        branches: [
          for (final (path, screen) in const [
            ('/home', HomeScreen()),
            ('/activity', ActivityScreen()),
            ('/signers', SignersScreen()),
            ('/settings', SettingsScreen()),
          ])
            StatefulShellBranch(
              routes: [
                GoRoute(
                  path: path,
                  pageBuilder: (_, _) => NoTransitionPage(child: screen),
                ),
              ],
            ),
        ],
      ),
      GoRoute(
        path: '/receive',
        pageBuilder: (_, _) => page(const ReceiveScreen()),
      ),
      GoRoute(
        path: '/received/:txid',
        pageBuilder: (_, state) =>
            page(ReceivedScreen(txid: state.pathParameters['txid']!)),
      ),
      GoRoute(
        path: '/export',
        pageBuilder: (_, _) => page(const SecureScreen(child: ExportScreen())),
      ),
      GoRoute(
        path: '/backup-prompt',
        pageBuilder: (_, _) => page(const BackupPromptScreen()),
      ),
      GoRoute(
        path: '/restore',
        pageBuilder: (_, _) => page(const SecureScreen(child: RestoreScreen())),
      ),
      GoRoute(
        path: '/recover',
        pageBuilder: (_, _) => page(const SecureScreen(child: RecoverScreen())),
      ),
      GoRoute(
        path: '/replace-signer',
        pageBuilder: (_, state) {
          final args = state.extra as (String, String)?;
          return page(
            SecureScreen(
              child: ReplaceSignerScreen(signer: args?.$1, code: args?.$2),
            ),
          );
        },
      ),
      GoRoute(
        path: '/diagnostics',
        pageBuilder: (_, _) => page(const DiagnosticsScreen()),
      ),
      GoRoute(
        path: '/status',
        pageBuilder: (_, _) => page(const SyncStatusScreen()),
      ),
      GoRoute(
        path: '/vault-server',
        pageBuilder: (_, _) => page(const VaultServerScreen()),
        routes: [
          GoRoute(
            path: 'follow',
            pageBuilder: (_, state) => page(
              FollowMoveScreen(link: state.uri.queryParameters['link'] ?? ''),
            ),
          ),
        ],
      ),
      GoRoute(
        path: '/vaults',
        pageBuilder: (_, _) => page(const ManageVaultsScreen()),
      ),
      GoRoute(
        path: '/vault-protection',
        pageBuilder: (_, _) => page(const VaultProtectionScreen()),
      ),
      GoRoute(
        path: '/viewing-key',
        pageBuilder: (_, _) =>
            page(const SecureScreen(child: ViewingKeyScreen())),
      ),
      GoRoute(
        path: '/payment-request',
        pageBuilder: (_, state) => switch (state.extra) {
          final PendingPaymentLink link => page(
            PaymentRequestScreen(link: link),
          ),
          _ => page(const SizedBox.shrink()),
        },
      ),
      GoRoute(
        path: '/send',
        pageBuilder: (_, state) =>
            page(SendScreen(prefill: state.extra as SendPrefill?)),
      ),
      GoRoute(
        path: '/proposal/:id',
        pageBuilder: (_, state) =>
            page(ProposalScreen(id: state.pathParameters['id']!)),
        routes: [
          GoRoute(
            path: 'send',
            pageBuilder: (_, state) =>
                page(SendingScreen(id: state.pathParameters['id']!)),
          ),
        ],
      ),
    ],
  );
});

class ZafeApp extends ConsumerWidget {
  const ZafeApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeModeProvider);
    ref.watch(mempoolWatchProvider);
    ref.watch(torLifecycleProvider);
    ref.watch(appUpdateProvider);
    return AppThemeHost(
      themeMode: themeMode,
      child: MaterialApp.router(
        title: 'Zafe',
        debugShowCheckedModeBanner: false,
        theme: buildMaterialLightTheme(),
        darkTheme: buildMaterialDarkTheme(),
        themeMode: themeMode,
        routerConfig: ref.watch(_routerProvider),
        builder: (context, child) => AppLockGate(child: child!),
      ),
    );
  }
}

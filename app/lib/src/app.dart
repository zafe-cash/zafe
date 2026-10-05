import 'core/platform/secure_screen.dart';
import 'dart:async';

import 'package:flutter/cupertino.dart' show CupertinoPage;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

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
import 'features/received/received_screen.dart';
import 'core/widgets/app_icon.dart';
import 'core/widgets/app_toast.dart';
import 'features/send/payment_link.dart';
import 'features/send/payment_request_screen.dart';
import 'features/send/send_screen.dart';
import 'features/settings/settings_screen.dart';
import 'features/recover/recover_screen.dart';
import 'features/settings/vault_protection_screen.dart';
import 'features/signers/replace_signer_screen.dart';
import 'features/settings/viewing_key_screen.dart';
import 'providers/device_lock_provider.dart';
import 'providers/tor_provider.dart';
import 'providers/mempool_watch_provider.dart';
import 'providers/server_failover_provider.dart';
import 'providers/theme_mode_provider.dart';
import 'notifications/vault_updates.dart' show kReceivedPrefix, kSeatMovePrefix;
import 'notifications/vault_watch.dart';
import 'providers/vault_provider.dart';
import 'services/app_update.dart';
import 'services/invite_links.dart';

final _routerProvider = Provider<GoRouter>((ref) {
  final vault = ref.read(vaultProvider);
  final initial = vault.hasVault
      ? '/home'
      : vault.isSettingUp
      ? '/setup'
      : '/welcome';

  Page<void> page(Widget child) => CupertinoPage(child: child);

  late final GoRouter router;
  // Tapped notifications open their payment (also the one that launched the app),
  // switching to that payment's vault first.
  Future<void> openTapped() async {
    final payload = notificationTaps.value;
    final ids = payload == null ? null : parsePayload(payload);
    if (ids == null) return;
    notificationTaps.value = null;
    final (vaultId, proposalId) = ids;
    final vaults = ref.read(vaultProvider);
    if (!vaults.vaults.any((v) => v.id == vaultId && v.ready)) return;
    if (vaults.activeId != vaultId || vaults.isAdding) {
      await ref.read(vaultProvider.notifier).switchTo(vaultId);
      router.go('/home');
    }
    if (proposalId.startsWith(kSeatMovePrefix)) {
      router.go('/signers'); // a tab: go, never push
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

  // Invite links open the Join screen with the invite filled in; joining still takes a
  // tap there and the safety number check after it. From inside a vault it's "Add vault".
  // A link that arrives during key generation or the backup prompt right after it waits
  // for the next navigation away from them.
  void openInviteLink() {
    final invite = inviteLinks.value;
    if (invite == null) return;
    final vaults = ref.read(vaultProvider);
    if (vaults.isSettingUp && (vaults.membership?.sealed ?? false)) return;
    final here = router.routerDelegate.currentConfiguration.uri.path;
    if (here == '/backup-prompt') return;
    inviteLinks.value = null;
    if (vaults.activeId != null) {
      ref.read(vaultProvider.notifier).beginAddVault();
    }
    router.go('/welcome'); // so back from Join lands on onboarding
    router.push(
      Uri(path: '/join', queryParameters: {'invite': invite}).toString(),
    );
  }

  inviteLinks.addListener(openInviteLink);
  ref.onDispose(() => inviteLinks.removeListener(openInviteLink));
  WidgetsBinding.instance.addPostFrameCallback((_) {
    void retry() => scheduleMicrotask(openInviteLink);
    router.routerDelegate.addListener(retry);
    ref.onDispose(() => router.routerDelegate.removeListener(retry));
    openInviteLink();
  });

  // Payment links (`zcash:`) open the payment request screen, which warns, shows the
  // details and lets the owner pick a vault; nothing is proposed without their tick and
  // the send flow after it. A link waits while the app is locked, during key generation
  // and on the backup prompt, and is dropped once it waited past `kPaymentLinkTtl`.
  void openPaymentLink() {
    final link = paymentLinks.value;
    if (link == null) return;
    void tell(String message) {
      final context = router.routerDelegate.navigatorKey.currentContext;
      if (context == null) return;
      showAppToast(
        context,
        message,
        iconName: AppIcons.warningCircle,
        duration: const Duration(seconds: 4),
      );
    }

    if (paymentLinkExpired(link, DateTime.now())) {
      paymentLinks.value = null;
      tell(kPaymentLinkExpiredMessage);
      return;
    }
    if (ref.read(appLockedProvider)) return;
    final vaults = ref.read(vaultProvider);
    if (!vaults.vaults.any((v) => v.ready)) {
      paymentLinks.value = null;
      tell(kPaymentLinkNoVaultMessage);
      return;
    }
    if (vaults.isSettingUp && (vaults.membership?.sealed ?? false)) return;
    final here = router.routerDelegate.currentConfiguration.uri.path;
    if (here == '/backup-prompt') return;
    paymentLinks.value = null;
    // A newer link replaces the one on screen.
    here == '/payment-request'
        ? router.pushReplacement('/payment-request', extra: link)
        : router.push('/payment-request', extra: link);
  }

  paymentLinks.addListener(openPaymentLink);
  ref.onDispose(() => paymentLinks.removeListener(openPaymentLink));
  ref.listen(appLockedProvider, (_, _) => scheduleMicrotask(openPaymentLink));
  // Sync moved to another listed Zcash server: say so once.
  ref.listen(serverFailoverProvider, (_, moved) {
    final context = router.routerDelegate.navigatorKey.currentContext;
    if (moved == null || context == null) return;
    showAppToast(
      context,
      moved.message,
      iconName: AppIcons.warningCircle,
      duration: const Duration(seconds: 4),
    );
  });
  WidgetsBinding.instance.addPostFrameCallback((_) {
    void retry() => scheduleMicrotask(openPaymentLink);
    router.routerDelegate.addListener(retry);
    ref.onDispose(() => router.routerDelegate.removeListener(retry));
    openPaymentLink();
  });

  return router = GoRouter(
    initialLocation: initial,
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
        '/replace-signer',
        '/export',
        '/activity',
        '/backup-prompt',
        '/scan-recipient',
        '/scan-recovery',
      ].any(loc.startsWith);
      // Picks its own vault, so it also opens while another one is being added.
      if (loc == '/payment-request') return null;
      if (vault.hasVault && !inVault) return '/home';
      if (!vault.hasVault && inVault) {
        return vault.isSettingUp ? '/setup' : '/welcome';
      }
      return null;
    },
    routes: [
      GoRoute(
        path: '/welcome',
        pageBuilder: (_, _) => page(const WelcomeScreen()),
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
        pageBuilder: (_, _) => page(const ScanInviteScreen()),
      ),
      GoRoute(
        path: '/scan-recovery',
        pageBuilder: (_, _) => page(const ScanRecoveryScreen()),
      ),
      GoRoute(
        path: '/scan-recipient',
        pageBuilder: (_, _) =>
            page(const ScanRecipientScreen(network: kZafeNetwork)),
      ),
      // Screens showing invites or backups block screenshots (SecureScreen).
      GoRoute(
        path: '/setup',
        pageBuilder: (_, _) => page(const SecureScreen(child: SetupScreen())),
      ),
      // The vault's tabs; everything else is pushed over the tab bar.
      StatefulShellRoute.indexedStack(
        builder: (_, _, shell) => VaultShell(shell: shell),
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
          return page(ReplaceSignerScreen(signer: args?.$1, code: args?.$2));
        },
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
        pageBuilder: (_, state) => page(
          PaymentRequestScreen(link: state.extra! as PendingPaymentLink),
        ),
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
    // Pending incoming payments while the app is open (starts and stops by itself).
    ref.watch(mempoolWatchProvider);
    // "Use Tor": connect at launch, dormant in the background.
    ref.watch(torLifecycleProvider);
    // Play in-app updates: checks at launch and on resume (Android, Play installs).
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
        // Over every route, sheet and toast: the app lock.
        builder: (context, child) => AppLockGate(child: child!),
      ),
    );
  }
}

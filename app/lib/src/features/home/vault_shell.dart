import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/app_mobile_shell.dart';
import '../../core/layout/mobile/app_mobile_tab_bar.dart';
import '../../core/widgets/app_icon.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../backup/backup_prompt_screen.dart' show backupStatusProvider;

/// The vault's tabs (Home, Activity, Signers, Settings) under the floating tab bar.
/// Pushed pages (a payment, Send, Receive...) cover the bar.
class VaultShell extends ConsumerWidget {
  const VaultShell({required this.shell, super.key});
  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // A dot on Signers only for what waits on you there: a seat move to approve, or
    // this phone's own backup.
    final me = ref.watch(vaultProvider).myKeyHex;
    final moves = ref.watch(proposalsProvider).seatMoves;
    final signers =
        moves.any((m) => m.oldKeyHex != me && !m.approvals.contains(me)) ||
        ref.watch(backupStatusProvider).value == false;
    return AppMobileShell(
      body: shell,
      tabBar: AppMobileTabBar(
        items: [
          const AppMobileTabItem(iconName: AppIcons.home, label: 'Home'),
          const AppMobileTabItem(iconName: AppIcons.history, label: 'Activity'),
          AppMobileTabItem(
            iconName: AppIcons.users,
            label: 'Signers',
            badge: signers,
          ),
          const AppMobileTabItem(iconName: AppIcons.cog, label: 'Settings'),
        ],
        currentIndex: shell.currentIndex,
        // Selecting the current tab again goes back to its root.
        onSelect: (i) =>
            shell.goBranch(i, initialLocation: i == shell.currentIndex),
      ),
    );
  }
}

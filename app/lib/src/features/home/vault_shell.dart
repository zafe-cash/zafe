import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/app_mobile_shell.dart';
import '../../core/layout/mobile/app_mobile_tab_bar.dart';
import '../../core/widgets/app_icon.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../backup/backup_prompt_screen.dart' show backupStatusProvider;

class VaultShell extends ConsumerWidget {
  const VaultShell({required this.shell, super.key});
  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final me = ref.watch(vaultProvider).myKeyHex;
    final proposals = ref.watch(proposalsProvider);
    final moves = proposals.seatMoves;
    final signers =
        moves.any((m) => m.oldKeyHex != me && !m.approvals.contains(me)) ||
        proposals.myPendingSeat(me) != null ||
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
        onSelect: (i) =>
            shell.goBranch(i, initialLocation: i == shell.currentIndex),
      ),
    );
  }
}

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_switch.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_provider.dart';

/// "Use Tor" in Settings: a switch on the row, with the connection's state beside it and
/// a way to retry when it failed. No sheet, no confirmation: it's a privacy preference
/// that can be switched back at any time.
class TorSettingRow extends ConsumerWidget {
  const TorSettingRow({super.key, required this.rowStyle});
  final TextStyle rowStyle;

  Future<void> _set(WidgetRef ref, bool enabled) async {
    await ref.read(torProvider.notifier).setEnabled(enabled);
    await _refreshVault(ref);
  }

  Future<void> _retry(WidgetRef ref) async {
    await ref.read(torProvider.notifier).retry();
    await _refreshVault(ref);
  }

  /// Picks up the new route right away instead of at the next poll.
  Future<void> _refreshVault(WidgetRef ref) async {
    if (ref.read(torProvider).failed) return;
    await ref.read(proposalsProvider.notifier).refreshFresh();
    await ref.read(vaultProvider.notifier).sync(force: true);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final tor = ref.watch(torProvider);
    // The switch says on or off; only a state in between needs words.
    final status = tor.failed
        ? 'Couldn\'t connect'
        : tor.connecting
        ? 'Connecting...'
        : null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MobileListRow(
          leading: AppIcon(AppIcons.tor, size: 20, color: colors.icon.muted),
          label: 'Use Tor',
          value: status,
          textStyle: rowStyle,
          valueTextStyle: rowStyle,
          valueColor: tor.failed
              ? colors.text.destructive
              : colors.text.secondary,
          trailing: AppSwitch(
            value: tor.enabled,
            busy: tor.connecting,
            semanticsLabel: 'Use Tor',
            onChanged: (v) => _set(ref, v),
          ),
        ),
        if (tor.failed)
          Padding(
            padding: const EdgeInsets.only(left: 36, bottom: AppSpacing.xs),
            child: Align(
              alignment: Alignment.centerLeft,
              child: AppTappable(
                semanticsLabel: 'Try Tor again',
                onTap: () => _retry(ref),
                child: Text(
                  'Try again',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.brand,
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

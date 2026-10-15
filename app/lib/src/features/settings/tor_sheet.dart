import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_provider.dart';

/// "Use Tor": what it does, the connection's state, and the switch.
Future<void> showTorSheet(BuildContext context) => showAppMobileSheet<void>(
  context: context,
  builder: (_) => const TorSheet(),
);

class TorSheet extends ConsumerWidget {
  const TorSheet({super.key});

  Future<void> _set(BuildContext context, WidgetRef ref, bool enabled) async {
    // Turning Tor off shows this phone's address to the servers: ask the owner.
    if (!enabled &&
        ref.read(vaultProvider).vaults.isNotEmpty &&
        !await confirmUnlock(
          context,
          ref,
          reason: 'Confirm it\'s you to stop using Tor',
          always: true,
        )) {
      return;
    }
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
    await ref.read(proposalsProvider.notifier).refresh();
    await ref.read(vaultProvider.notifier).sync(force: true);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final tor = ref.watch(torProvider);
    return MobileModalScaffold(
      title: 'Use Tor',
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            tor.explanation,
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          DetailRow(label: 'Status', value: tor.statusLabel),
          if (tor.failed && tor.error != null) ...[
            const SizedBox(height: AppSpacing.xs),
            SelectableText(
              tor.error!,
              maxLines: 3,
              style: AppTypography.codeSmall.copyWith(
                color: colors.text.secondary,
              ),
            ),
          ],
          const SizedBox(height: AppSpacing.md),
          if (!tor.enabled)
            AppButton(
              expand: true,
              onPressed: () => _set(context, ref, true),
              child: const Text('Turn on Tor'),
            )
          else ...[
            if (tor.failed) ...[
              AppButton(
                expand: true,
                onPressed: () => _retry(ref),
                child: const Text('Try again'),
              ),
              const SizedBox(height: AppSpacing.xs),
            ] else if (tor.connecting) ...[
              const AppButton(
                expand: true,
                onPressed: null,
                child: Text('Connecting…'),
              ),
              const SizedBox(height: AppSpacing.xs),
            ],
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: () => _set(context, ref, false),
              child: const Text('Turn off Tor'),
            ),
          ],
        ],
      ),
    );
  }
}

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/storage/vault_summaries.dart';
import 'vault_provider.dart';

/// Whether a vault other than the open one has a payment waiting for this member, from
/// each vault's summary (written by the app's refreshes, background checks and the check
/// a push runs while the app is open). Home shows it as a dot on the vault picture, like
/// the vault switcher's.
final otherVaultsWaitingProvider = FutureProvider.autoDispose<bool>((
  ref,
) async {
  final others = ref.watch(
    vaultProvider.select(
      (v) => [
        for (final x in v.vaults)
          if (x.id != v.activeId) x.id,
      ].join(','),
    ),
  );
  void reread() => ref.invalidateSelf();
  VaultSummaries.changes.addListener(reread);
  ref.onDispose(() => VaultSummaries.changes.removeListener(reread));
  if (others.isEmpty) return false;
  return VaultSummaries.anyWaiting(others.split(','));
});

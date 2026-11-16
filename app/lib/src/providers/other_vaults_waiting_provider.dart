import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/storage/vault_summaries.dart';
import 'vault_provider.dart';

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

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/errors/zafe_error_copy.dart';
import '../core/storage/zafe_paths.dart';
import '../notifications/vault_watch.dart' show recordSeen;
import '../rust/api/spends.dart' as rust;
import 'endpoints_provider.dart';
import 'vault_provider.dart';
import '../core/storage/wallet_key.dart';

/// Vault spends the log doesn't account for (spec §10.4.4): money that left without an
/// approved payment, so keys were used outside Zafe. Empty when all is well. Reloads after
/// every wallet sync (a new balance or height) and when another vault becomes active.
class UnapprovedSpendsNotifier extends Notifier<rust.UnapprovedSpends> {
  bool _loading = false;
  bool _again = false;

  @override
  rust.UnapprovedSpends build() {
    ref.watch(vaultProvider.select((v) => v.activeId));
    ref.listen(
      vaultProvider.select((v) => v.balance),
      (_, _) => refresh(),
      fireImmediately: true,
    );
    return const rust.UnapprovedSpends(spends: []);
  }

  Future<void> refresh() async {
    final vault = ref.read(vaultProvider);
    final material = vault.material;
    final seeds = vault.identity;
    final vaultId = vault.activeId;
    if (material == null || seeds == null || vaultId == null) return;
    if (_loading) {
      _again = true;
      return;
    }
    _loading = true;
    try {
      final paths = await ZafePaths.get();
      // While the relay is down the check runs on this phone's copy of the history
      // (`fromCopyAt`), and the alert says so.
      final result = await rust.unapprovedSpends(
        relayUrl: ref.read(endpointsProvider).relayUrl,
        dbDir: paths.dbDir,
        dbKey: walletKeyFor(seeds, vaultId),
        seeds: seeds,
        material: material,
      );
      final items = result.spends;
      if (ref.read(vaultProvider).activeId == vaultId) {
        state = result;
        // Seen on screen: the background check doesn't announce it again.
        if (WidgetsBinding.instance.lifecycleState ==
            AppLifecycleState.resumed) {
          await recordSeen(vaultId, null, unapprovedSpends: items);
        }
      }
    } catch (e) {
      // The relay or the wallet isn't reachable right now: keep what we showed.
      debugPrint('unapproved spends check failed: ${describeError(e)}');
    } finally {
      _loading = false;
    }
    if (_again) {
      _again = false;
      await refresh();
    }
  }
}

final unapprovedSpendsProvider =
    NotifierProvider<UnapprovedSpendsNotifier, rust.UnapprovedSpends>(
      UnapprovedSpendsNotifier.new,
    );

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/errors/zafe_error_copy.dart';
import '../core/feedback/payment_feedback.dart';
import '../core/storage/zafe_paths.dart';
import '../notifications/vault_watch.dart' show recordSeen;
import '../rust/api/received.dart' as rust;
import 'payment_sounds_provider.dart';
import 'vault_provider.dart';
import '../core/storage/wallet_key.dart';

class ReceivedState {
  const ReceivedState({this.items = const [], this.loaded = false});

  final List<rust.ReceivedInfo> items;
  final bool loaded;

  rust.ReceivedInfo? byTxid(String txid) {
    for (final r in items) {
      if (r.txid == txid) return r;
    }
    return null;
  }
}

class ReceivedNotifier extends Notifier<ReceivedState> {
  bool _loading = false;
  bool _again = false;

  @override
  ReceivedState build() {
    ref.watch(vaultProvider.select((v) => v.activeId));
    ref.listen(
      vaultProvider.select((v) => v.balance),
      (_, _) => refresh(),
      fireImmediately: true,
    );
    return const ReceivedState();
  }

  Future<void> refresh() async {
    final vault = ref.read(vaultProvider);
    final material = vault.material;
    final vaultId = vault.activeId;
    if (material == null || vaultId == null) return;
    if (_loading) {
      _again = true;
      return;
    }
    _loading = true;
    try {
      final paths = await ZafePaths.get();
      final items = await rust.listReceived(
        dbDir: paths.dbDir,
        dbKey: walletKeyFor(ref.read(vaultProvider).identity!, vaultId),
        material: material,
      );
      if (ref.read(vaultProvider).activeId == vaultId) {
        final previous = state.loaded ? state.items.map((r) => r.txid) : null;
        state = ReceivedState(items: items, loaded: true);
        if (WidgetsBinding.instance.lifecycleState ==
            AppLifecycleState.resumed) {
          final fresh = items
              .where((r) => r.minedHeight == 0 || r.confirmations <= 2)
              .map((r) => r.txid);
          if (newArrivals(previous, fresh).isNotEmpty) {
            unawaited(
              PaymentFeedback.play(
                PaymentMoment.received,
                sound: ref.read(paymentSoundsProvider),
              ),
            );
          }
          await recordSeen(vaultId, null, received: items);
        }
      }
    } catch (e) {
      debugPrint('received payments failed: ${describeError(e)}');
    } finally {
      _loading = false;
    }
    if (_again) {
      _again = false;
      await refresh();
    }
  }
}

final receivedProvider = NotifierProvider<ReceivedNotifier, ReceivedState>(
  ReceivedNotifier.new,
);

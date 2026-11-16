import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/errors/zafe_error_copy.dart';
import '../core/storage/zafe_paths.dart';
import '../rust/api/mempool.dart' as rust;
import 'endpoints_provider.dart';
import 'mempool_watch_policy.dart';
import 'received_provider.dart';
import 'vault_provider.dart';
import '../core/storage/wallet_key.dart';

final mempoolWatchProvider = Provider<MempoolWatchController>((ref) {
  StreamSubscription<rust.MempoolEvent>? subscription;
  var disposed = false;

  Future<void> start(MempoolWatchTarget target, void Function() onEnded) async {
    final watchId = rust.beginMempoolWatch();
    final previous = subscription;
    subscription = null;
    try {
      final vault = ref.read(vaultProvider);
      final material = vault.material;
      if (vault.activeId != target.vaultId || material == null) {
        onEnded();
        return;
      }
      await previous?.cancel();
      final paths = await ZafePaths.get();
      final dbKey = walletKeyFor(vault.identity!, target.vaultId);
      if (disposed) return;
      final stream = rust.watchMempool(
        watchId: watchId,
        lightwalletdUrl: target.lightwalletdUrl,
        dbDir: paths.dbDir,
        dbKey: dbKey,
        material: material,
      );
      subscription = stream.listen(
        (event) {
          if (!disposed) _onEvent(ref, target, event);
        },
        onError: (Object e) =>
            debugPrint('mempool watch error: ${describeError(e)}'),
        onDone: onEnded,
        cancelOnError: false,
      );
    } catch (e) {
      debugPrint('mempool watch failed to start: ${describeError(e)}');
      onEnded();
    }
  }

  final controller = MempoolWatchController(
    start: (target, onEnded) => unawaited(start(target, onEnded)),
    stop: rust.stopMempoolWatch,
  );

  void refreshTarget([AppLifecycleState? state]) {
    final vault = ref.read(vaultProvider);
    controller.update(
      mempoolWatchTarget(
        foreground: isForeground(
          state ?? WidgetsBinding.instance.lifecycleState,
        ),
        vaultId: vault.activeId,
        synced: vault.balance != null,
        lightwalletdUrl: ref.read(endpointsProvider).lightwalletdUrl,
      ),
    );
  }

  final lifecycle = AppLifecycleListener(onStateChange: refreshTarget);
  ref.listen(
    vaultProvider.select((v) => (v.activeId, v.balance != null)),
    (_, _) => refreshTarget(),
  );
  ref.listen(
    endpointsProvider.select((e) => e.lightwalletdUrl),
    (_, _) => refreshTarget(),
  );
  ref.onDispose(() {
    disposed = true;
    lifecycle.dispose();
    controller.dispose();
    unawaited(subscription?.cancel());
  });
  refreshTarget();
  return controller;
});

void _onEvent(Ref ref, MempoolWatchTarget target, rust.MempoolEvent event) {
  switch (event.status) {
    case rust.MempoolStatus.stored:
      if (ref.read(vaultProvider).activeId == target.vaultId) {
        unawaited(ref.read(receivedProvider.notifier).refresh());
        final vault = ref.read(vaultProvider.notifier)..markDirty();
        unawaited(vault.sync());
      }
    case rust.MempoolStatus.storeFailed ||
        rust.MempoolStatus.disconnected ||
        rust.MempoolStatus.failed:
      final error = event.error;
      debugPrint(
        'mempool watch ${event.status.name}'
        '${error == null ? '' : ': ${describeError(error)}'}',
      );
    case rust.MempoolStatus.connected:
      break;
  }
}

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show PlatformInt64Util;

import '../core/errors/zafe_error_copy.dart' show describeError;
import '../rust/api/watch.dart' as rust;

class LiveActivityPolicy {
  static const fallbackPoll = Duration(seconds: 15);

  static const livePoll = Duration(seconds: 60);

  bool _live = false;

  bool get live => _live;

  Duration get pollInterval => _live ? livePoll : fallbackPoll;

  bool onEvent(rust.VaultActivityKind kind) {
    switch (kind) {
      case rust.VaultActivityKind.connected:
      case rust.VaultActivityKind.activity:
        _live = true;
        return true;
      case rust.VaultActivityKind.failed:
      case rust.VaultActivityKind.unsupported:
        _live = false;
        return false;
    }
  }

  void onEnded() => _live = false;
}

typedef WatchStarter =
    Stream<rust.VaultActivity> Function({
      required String relayUrl,
      required List<int> seeds,
      required List<int> material,
      required int watchId,
    });
typedef WatchStopper = void Function(int watchId);

Stream<rust.VaultActivity> _rustStart({
  required String relayUrl,
  required List<int> seeds,
  required List<int> material,
  required int watchId,
}) => rust.watchVault(
  relayUrl: relayUrl,
  seeds: seeds,
  material: material,
  watchId: PlatformInt64Util.from(watchId),
);

void _rustStop(int watchId) =>
    rust.stopVaultWatch(watchId: PlatformInt64Util.from(watchId));

class LiveVaultWatch {
  LiveVaultWatch({
    required this.onRefresh,
    required this.onPollIntervalChanged,
    this.onInbox,
    WatchStarter? starter,
    WatchStopper? stopper,
  }) : _starter = starter ?? _rustStart,
       _stopper = stopper ?? _rustStop;

  final void Function() onRefresh;

  final void Function(Duration interval) onPollIntervalChanged;

  final void Function()? onInbox;

  final WatchStarter _starter;
  final WatchStopper _stopper;
  final policy = LiveActivityPolicy();

  StreamSubscription<rust.VaultActivity>? _subscription;
  int? _watchId;
  static int _lastId = 0;

  static int _nextId() {
    final now = DateTime.now().microsecondsSinceEpoch;
    _lastId = now > _lastId ? now : _lastId + 1;
    return _lastId;
  }

  bool get running => _subscription != null;

  Duration get pollInterval => policy.pollInterval;

  void start({
    required String relayUrl,
    required List<int> seeds,
    required List<int> material,
  }) {
    stop();
    final id = _nextId();
    _watchId = id;
    _subscription =
        _starter(
          relayUrl: relayUrl,
          seeds: seeds,
          material: material,
          watchId: id,
        ).listen(
          (event) => _onEvent(id, event),
          onError: (Object e) {
            debugPrint('vault watch failed: ${describeError(e)}');
            _ended(id);
          },
          onDone: () => _ended(id),
        );
  }

  void stop() {
    final id = _watchId;
    final subscription = _subscription;
    _watchId = null;
    _subscription = null;
    if (id != null) _stopper(id);
    unawaited(subscription?.cancel());
    _setLive(() => policy.onEnded());
  }

  void _onEvent(int id, rust.VaultActivity event) {
    if (id != _watchId) return;
    if (event.kind == rust.VaultActivityKind.failed) {
      final error = event.error;
      debugPrint(
        'vault watch: ${error == null ? 'failed' : describeError(error)} '
        '(retry in ${event.retryInSecs} s)',
      );
    }
    var refresh = false;
    _setLive(() => refresh = policy.onEvent(event.kind));
    if (event.inbox) onInbox?.call();
    if (refresh) onRefresh();
  }

  void _ended(int id) {
    if (id != _watchId) return;
    _watchId = null;
    _subscription = null;
    _setLive(() => policy.onEnded());
  }

  void _setLive(void Function() change) {
    final before = policy.pollInterval;
    change();
    if (policy.pollInterval != before) {
      onPollIntervalChanged(policy.pollInterval);
    }
  }
}

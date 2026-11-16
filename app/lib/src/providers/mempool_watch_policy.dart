import 'dart:async';

import 'package:flutter/widgets.dart';

@immutable
class MempoolWatchTarget {
  const MempoolWatchTarget({
    required this.vaultId,
    required this.lightwalletdUrl,
  });

  final String vaultId;
  final String lightwalletdUrl;

  @override
  bool operator ==(Object other) =>
      other is MempoolWatchTarget &&
      other.vaultId == vaultId &&
      other.lightwalletdUrl == lightwalletdUrl;

  @override
  int get hashCode => Object.hash(vaultId, lightwalletdUrl);

  @override
  String toString() => 'MempoolWatchTarget($vaultId, $lightwalletdUrl)';
}

bool isForeground(AppLifecycleState? state) =>
    state == null ||
    state == AppLifecycleState.resumed ||
    state == AppLifecycleState.inactive;

MempoolWatchTarget? mempoolWatchTarget({
  required bool foreground,
  required String? vaultId,
  required bool synced,
  required String lightwalletdUrl,
}) {
  if (!foreground || vaultId == null || !synced) return null;
  return MempoolWatchTarget(vaultId: vaultId, lightwalletdUrl: lightwalletdUrl);
}

typedef StartMempoolWatch =
    void Function(MempoolWatchTarget target, void Function() onEnded);

class MempoolWatchController {
  MempoolWatchController({
    required this.start,
    required this.stop,
    this.retryDelay = const Duration(seconds: 10),
    Timer Function(Duration, void Function())? schedule,
  }) : _schedule = schedule ?? Timer.new;

  final StartMempoolWatch start;
  final void Function() stop;
  final Duration retryDelay;
  final Timer Function(Duration, void Function()) _schedule;

  MempoolWatchTarget? _desired;
  MempoolWatchTarget? _running;
  int _session = 0;
  Timer? _retry;
  bool _disposed = false;

  MempoolWatchTarget? get running => _running;

  void update(MempoolWatchTarget? desired) {
    if (_disposed) return;
    _desired = desired;
    _retry?.cancel();
    _retry = null;
    if (desired == _running) return;
    if (_running != null) {
      _running = null;
      _session++;
      stop();
    }
    if (desired != null) {
      _running = desired;
      final session = ++_session;
      start(desired, () => _ended(session));
    }
  }

  void _ended(int session) {
    if (_disposed || session != _session || _running == null) return;
    _running = null;
    _retry?.cancel();
    _retry = _schedule(retryDelay, () {
      _retry = null;
      update(_desired);
    });
  }

  void dispose() {
    update(null);
    _disposed = true;
  }
}

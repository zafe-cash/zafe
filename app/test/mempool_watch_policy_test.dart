import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/providers/mempool_watch_policy.dart';

const _a = MempoolWatchTarget(vaultId: 'a', lightwalletdUrl: 'http://l');
const _b = MempoolWatchTarget(vaultId: 'b', lightwalletdUrl: 'http://l');

class _FakeTimer implements Timer {
  _FakeTimer(this.duration, this.callback);
  final Duration duration;
  final void Function() callback;
  bool cancelled = false;

  @override
  void cancel() => cancelled = true;

  @override
  bool get isActive => !cancelled;

  @override
  int get tick => 0;

  void fire() {
    if (!cancelled) callback();
  }
}

class _Harness {
  final log = <String>[];
  final ended = <void Function()>[];
  final timers = <_FakeTimer>[];
  late final controller = MempoolWatchController(
    start: (t, onEnded) {
      log.add('start ${t.vaultId}');
      ended.add(onEnded);
    },
    stop: () => log.add('stop'),
    retryDelay: const Duration(seconds: 10),
    schedule: (d, f) {
      final t = _FakeTimer(d, f);
      timers.add(t);
      return t;
    },
  );
}

void main() {
  group('mempoolWatchTarget', () {
    test('only in the foreground, for a synced active vault', () {
      MempoolWatchTarget? target({
        bool foreground = true,
        String? vaultId = 'a',
        bool synced = true,
      }) => mempoolWatchTarget(
        foreground: foreground,
        vaultId: vaultId,
        synced: synced,
        lightwalletdUrl: 'http://l',
      );
      expect(target(), _a);
      expect(target(foreground: false), isNull);
      expect(target(vaultId: null), isNull);
      expect(target(synced: false), isNull);
    });

    test('foreground states', () {
      expect(isForeground(null), isTrue);
      expect(isForeground(AppLifecycleState.resumed), isTrue);
      expect(isForeground(AppLifecycleState.inactive), isTrue);
      expect(isForeground(AppLifecycleState.hidden), isFalse);
      expect(isForeground(AppLifecycleState.paused), isFalse);
      expect(isForeground(AppLifecycleState.detached), isFalse);
    });

    test('targets differ by vault and by server', () {
      expect(_a == _b, isFalse);
      expect(
        _a ==
            const MempoolWatchTarget(
              vaultId: 'a',
              lightwalletdUrl: 'https://other',
            ),
        isFalse,
      );
      expect(
        _a ==
            const MempoolWatchTarget(vaultId: 'a', lightwalletdUrl: 'http://l'),
        isTrue,
      );
    });
  });

  group('MempoolWatchController', () {
    test('starts once, ignores repeats, stops when backgrounded', () {
      final h = _Harness();
      h.controller.update(_a);
      h.controller.update(_a);
      expect(h.log, ['start a']);
      expect(h.controller.running, _a);
      h.controller.update(null);
      h.controller.update(null);
      expect(h.log, ['start a', 'stop']);
      expect(h.controller.running, isNull);
    });

    test('a vault switch stops the old watch before starting the new one', () {
      final h = _Harness();
      h.controller.update(_a);
      h.controller.update(_b);
      expect(h.log, ['start a', 'stop', 'start b']);
      expect(h.controller.running, _b);
    });

    test('the end of a stopped watch is ignored', () {
      final h = _Harness();
      h.controller.update(_a);
      h.controller.update(_b);
      h.ended.first();
      expect(h.controller.running, _b);
      expect(h.timers, isEmpty);
    });

    test('a watch that ends by itself restarts after the delay', () {
      final h = _Harness();
      h.controller.update(_a);
      h.ended.single();
      expect(h.controller.running, isNull);
      expect(h.timers.single.duration, const Duration(seconds: 10));
      h.timers.single.fire();
      expect(h.log, ['start a', 'start a']);
      expect(h.controller.running, _a);
    });

    test('no restart once no longer wanted', () {
      final h = _Harness();
      h.controller.update(_a);
      h.ended.single();
      h.controller.update(null);
      expect(h.timers.single.cancelled, isTrue);
      h.timers.single.fire();
      expect(h.log, ['start a']);
      expect(h.controller.running, isNull);
    });

    test('the retry follows the latest target', () {
      final h = _Harness();
      h.controller.update(_a);
      h.ended.single();
      h.controller.update(_b);
      expect(h.log, ['start a', 'start b']);
      expect(h.timers.single.cancelled, isTrue);
    });

    test('dispose stops the watch and ignores later calls', () {
      final h = _Harness();
      h.controller.update(_a);
      h.controller.dispose();
      h.controller.update(_b);
      h.ended.single();
      expect(h.log, ['start a', 'stop']);
      expect(h.timers, isEmpty);
    });
  });
}

import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/rust/api/watch.dart';
import 'package:zafe/src/services/live_vault_watch.dart';

VaultActivity event(
  VaultActivityKind kind, {
  int retry = 0,
  bool inbox = false,
}) => VaultActivity(kind: kind, inbox: inbox, retryInSecs: retry);

/// A fake Rust side: one stream controller per started watch.
class FakeRust {
  final started = <int, StreamController<VaultActivity>>{};
  final stopped = <int>[];

  Stream<VaultActivity> start({
    required String relayUrl,
    required List<int> seeds,
    required List<int> material,
    required int watchId,
  }) {
    final controller = StreamController<VaultActivity>();
    started[watchId] = controller;
    return controller.stream;
  }

  void stop(int watchId) => stopped.add(watchId);
}

void main() {
  group('LiveActivityPolicy', () {
    test('refreshes on activity and on connecting, not on failures', () {
      final policy = LiveActivityPolicy();
      expect(policy.live, isFalse);
      expect(policy.pollInterval, LiveActivityPolicy.fallbackPoll);

      expect(policy.onEvent(VaultActivityKind.connected), isTrue);
      expect(policy.live, isTrue);
      expect(policy.pollInterval, LiveActivityPolicy.livePoll);
      expect(policy.onEvent(VaultActivityKind.activity), isTrue);

      expect(policy.onEvent(VaultActivityKind.failed), isFalse);
      expect(policy.pollInterval, LiveActivityPolicy.fallbackPoll);
      expect(policy.onEvent(VaultActivityKind.activity), isTrue);
      expect(policy.live, isTrue, reason: 'back once events flow again');

      expect(policy.onEvent(VaultActivityKind.unsupported), isFalse);
      expect(policy.live, isFalse);
    });

    test('the poll always keeps running, slower while live', () {
      expect(
        LiveActivityPolicy.livePoll > LiveActivityPolicy.fallbackPoll,
        isTrue,
      );
      final policy = LiveActivityPolicy()
        ..onEvent(VaultActivityKind.connected)
        ..onEnded();
      expect(policy.pollInterval, LiveActivityPolicy.fallbackPoll);
    });
  });

  group('LiveVaultWatch', () {
    late FakeRust rust;
    late int refreshes;
    late int inboxes;
    late List<Duration> intervals;
    late LiveVaultWatch watch;

    setUp(() {
      rust = FakeRust();
      refreshes = 0;
      inboxes = 0;
      intervals = [];
      watch = LiveVaultWatch(
        onRefresh: () => refreshes++,
        onInbox: () => inboxes++,
        onPollIntervalChanged: intervals.add,
        starter: rust.start,
        stopper: rust.stop,
      );
    });

    void startWatch() =>
        watch.start(relayUrl: 'http://relay', seeds: [1], material: [2]);

    test('events refresh and relax the poll; failures restore it', () async {
      startWatch();
      final controller = rust.started.values.single;
      controller.add(event(VaultActivityKind.connected));
      await pumpEventQueue();
      expect(refreshes, 1);
      expect(intervals, [LiveActivityPolicy.livePoll]);

      controller.add(event(VaultActivityKind.activity));
      controller.add(event(VaultActivityKind.activity, inbox: true));
      await pumpEventQueue();
      expect(refreshes, 3);
      expect(inboxes, 1, reason: 'only the delivery');
      expect(intervals, [LiveActivityPolicy.livePoll], reason: 'unchanged');

      controller.add(event(VaultActivityKind.failed, retry: 4));
      await pumpEventQueue();
      expect(refreshes, 3);
      expect(intervals.last, LiveActivityPolicy.fallbackPoll);
      expect(watch.running, isTrue, reason: 'Rust retries by itself');
    });

    test('an older relay ends the watch and keeps the fallback poll', () async {
      startWatch();
      final controller = rust.started.values.single;
      controller.add(event(VaultActivityKind.unsupported));
      await controller.close();
      await pumpEventQueue();
      expect(watch.running, isFalse);
      expect(refreshes, 0);
      expect(intervals, isEmpty);
      expect(watch.pollInterval, LiveActivityPolicy.fallbackPoll);
    });

    test('stopping tells Rust and ignores late events', () async {
      startWatch();
      final id = rust.started.keys.single;
      final controller = rust.started[id]!;
      controller.add(event(VaultActivityKind.connected));
      await pumpEventQueue();

      watch.stop();
      expect(rust.stopped, [id]);
      expect(watch.running, isFalse);
      expect(intervals.last, LiveActivityPolicy.fallbackPoll);
      controller.add(event(VaultActivityKind.activity));
      await pumpEventQueue();
      expect(refreshes, 1, reason: 'nothing after stop');
    });

    test(
      'restarting (another vault) replaces the watch with a newer id',
      () async {
        startWatch();
        final first = rust.started.keys.single;
        startWatch();
        expect(rust.started.length, 2);
        final second = rust.started.keys.last;
        expect(second, greaterThan(first));
        expect(rust.stopped, [first]);

        // The old stream's end doesn't touch the new watch.
        await rust.started[first]!.close();
        rust.started[second]!.add(event(VaultActivityKind.connected));
        await pumpEventQueue();
        expect(watch.running, isTrue);
        expect(refreshes, 1);
      },
    );
  });
}

import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:zafe/src/core/errors/sync_failure.dart';
import 'package:zafe/src/core/network/tor_setting.dart';
import 'package:zafe/src/providers/tor_provider.dart';
import 'package:zafe/src/providers/vault_provider.dart';
import 'package:zafe/src/rust/api/error.dart';

/// Records what the notifier asks of Rust, in order.
class FakeTorRuntime implements TorRuntime {
  final calls = <String>[];
  TorConnection current = TorConnection.off;
  Future<TorConnection> Function()? onEnable;

  @override
  void request() {
    calls.add('request');
    if (current == TorConnection.off) current = TorConnection.connecting;
  }

  @override
  Future<TorConnection> enable({required int timeoutSecs}) async {
    calls.add('enable $timeoutSecs');
    final r = await (onEnable ?? () async => TorConnection.connected)();
    current = r;
    return r;
  }

  @override
  void disable() {
    calls.add('disable');
    current = TorConnection.off;
  }

  @override
  TorConnection state() => current;

  @override
  void setDormant(bool dormant) => calls.add('dormant $dormant');
}

ZafeError torErr(ZafeErrorKind kind) => ZafeError(
  kind: kind,
  message: 'detail',
  endpoint: ZafeEndpoint.none,
  members: const [],
);

ProviderContainer container(FakeTorRuntime runtime, {bool useTor = false}) {
  final c = ProviderContainer(
    overrides: [
      torRuntimeProvider.overrideWithValue(runtime),
      vaultBootstrapProvider.overrideWithValue(VaultBootstrap(useTor: useTor)),
    ],
  );
  addTearDown(c.dispose);
  return c;
}

void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('TorSetting', () {
    test('labels follow the choice, then the connection', () {
      expect(const TorSetting.off().statusLabel, 'Off');
      expect(const TorSetting.off().homeLabel, isNull);
      const on = TorSetting(
        enabled: true,
        connection: TorConnection.connecting,
      );
      expect(on.statusLabel, 'Connecting…');
      expect(on.homeLabel, 'Connecting to Tor…');
      expect(on.connecting, isTrue);
      final ok = on.copyWith(connection: TorConnection.connected);
      expect(ok.statusLabel, 'Connected');
      expect(ok.homeLabel, isNull);
      final failed = on.copyWith(connection: TorConnection.failed);
      expect(failed.statusLabel, 'Failed');
      expect(failed.homeLabel, 'Tor couldn\'t connect');
      expect(failed.failed, isTrue);
      // A choice that is on but whose runtime hasn't started still reads as connecting.
      expect(
        const TorSetting(
          enabled: true,
          connection: TorConnection.off,
        ).statusLabel,
        'Connecting…',
      );
    });
  });

  group('ensureTorForBackground', () {
    test('off: nothing to do, the check goes on', () async {
      final calls = <String>[];
      final go = await ensureTorForBackground(
        useTor: false,
        request: () => calls.add('request'),
        enable: () async {
          calls.add('enable');
          return TorConnection.connected;
        },
      );
      expect(go, isTrue);
      expect(calls, isEmpty);
    });

    test('on: the route switches before connecting', () async {
      final calls = <String>[];
      final go = await ensureTorForBackground(
        useTor: true,
        request: () => calls.add('request'),
        enable: () async {
          calls.add('enable');
          return TorConnection.connected;
        },
      );
      expect(go, isTrue);
      expect(calls, ['request', 'enable']);
    });

    test('a Tor that does not connect skips the check', () async {
      Object? reported;
      expect(
        await ensureTorForBackground(
          useTor: true,
          request: () {},
          enable: () async => throw torErr(ZafeErrorKind.torFailed),
          onError: (e) => reported = e,
        ),
        isFalse,
      );
      expect(reported, isA<ZafeError>());
      // Turned off meanwhile (another isolate): skip this run too.
      expect(
        await ensureTorForBackground(
          useTor: true,
          request: () {},
          enable: () async => TorConnection.off,
        ),
        isFalse,
      );
    });
  });

  group('TorNotifier', () {
    test('turning on switches the route first, saves, then connects', () async {
      final runtime = FakeTorRuntime();
      final c = container(runtime);
      expect(c.read(torProvider).enabled, isFalse);
      await c.read(torProvider.notifier).setEnabled(true);
      expect(runtime.calls, ['request', 'enable $kTorBootstrapTimeoutSecs']);
      expect(c.read(torProvider).connection, TorConnection.connected);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getBool(kUseTorKey), isTrue);
    });

    test('a failed bootstrap stays on Tor and can be retried', () async {
      final runtime = FakeTorRuntime()
        ..onEnable = () async => throw torErr(ZafeErrorKind.torFailed);
      final c = container(runtime);
      await c.read(torProvider.notifier).setEnabled(true);
      final failed = c.read(torProvider);
      expect(failed.enabled, isTrue);
      expect(failed.failed, isTrue);
      expect(failed.error, 'detail');
      expect(runtime.calls, isNot(contains('disable')));

      runtime.onEnable = () async => TorConnection.connected;
      await c.read(torProvider.notifier).retry();
      expect(c.read(torProvider).connection, TorConnection.connected);
      expect(c.read(torProvider).error, isNull);
    });

    test('turning off saves, then switches to direct', () async {
      SharedPreferences.setMockInitialValues({kUseTorKey: true});
      final runtime = FakeTorRuntime();
      final c = container(runtime, useTor: true);
      await c.read(torProvider.notifier).start();
      expect(c.read(torProvider).connection, TorConnection.connected);
      await c.read(torProvider.notifier).setEnabled(false);
      expect(runtime.calls.last, 'disable');
      expect(c.read(torProvider).enabled, isFalse);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getBool(kUseTorKey), isFalse);
    });

    test(
      'a bootstrap that ends after Tor was turned off changes nothing',
      () async {
        final runtime = FakeTorRuntime();
        final c = container(runtime, useTor: true);
        final gate = Completer<TorConnection>();
        runtime.onEnable = () => gate.future;
        final starting = c.read(torProvider.notifier).start();
        await Future<void>.delayed(Duration.zero);
        await c.read(torProvider.notifier).setEnabled(false);
        gate.complete(TorConnection.connected);
        await starting;
        expect(c.read(torProvider).enabled, isFalse);
        expect(c.read(torProvider).connection, TorConnection.off);
      },
    );

    test('off at launch: no bootstrap', () async {
      final runtime = FakeTorRuntime();
      final c = container(runtime);
      await c.read(torProvider.notifier).start();
      expect(runtime.calls, isEmpty);
    });

    test(
      'backgrounding makes Tor dormant; coming back retries a failure',
      () async {
        final runtime = FakeTorRuntime()
          ..onEnable = () async => throw torErr(ZafeErrorKind.torFailed);
        final c = container(runtime, useTor: true);
        await c.read(torProvider.notifier).start();
        expect(c.read(torProvider).failed, isTrue);
        c.read(torProvider.notifier).setForeground(false);
        expect(runtime.calls.last, 'dormant true');
        runtime.onEnable = () async => TorConnection.connected;
        c.read(torProvider.notifier).setForeground(true);
        await Future<void>.delayed(Duration.zero);
        await Future<void>.delayed(Duration.zero);
        expect(runtime.calls, contains('dormant false'));
        expect(c.read(torProvider).connection, TorConnection.connected);
      },
    );
  });

  group('sync failures', () {
    test('Tor errors are their own kinds and come first', () {
      final connecting = classifySyncFailure(
        torErr(ZafeErrorKind.torConnecting),
      );
      expect(connecting.kind, SyncFailureKind.torConnecting);
      expect(connecting.isTransient, isTrue);
      expect(connecting.endpoint, isNull);
      final failed = homeSyncFailure(
        syncError: ZafeError(
          kind: ZafeErrorKind.network,
          message: 'x',
          endpoint: ZafeEndpoint.lightwalletd,
          members: const [],
        ),
        relayError: torErr(ZafeErrorKind.torFailed),
      );
      expect(failed!.kind, SyncFailureKind.torFailed);
      expect(failed.isTransient, isFalse);
      expect(failed.suggestsSettings, isTrue);
    });
  });
}

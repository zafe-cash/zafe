import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/endpoints.dart';
import 'package:zafe/src/core/config/lightwalletd_presets.dart';
import 'package:zafe/src/core/config/network_config.dart';
import 'package:zafe/src/core/errors/sync_failure.dart';
import 'package:zafe/src/providers/server_failover_provider.dart';

void main() {
  test('each network lists its build default first, every URL valid and unique', () {
    expect(kMainnetLightwalletdPresets.first.url, kMainnetLightwalletdUrl);
    expect(kTestnetLightwalletdPresets.first.url, kTestnetLightwalletdUrl);
    expect(kRegtestLightwalletdPresets.first.url, kRegtestLightwalletdUrl);
    for (final list in [
      kMainnetLightwalletdPresets,
      kTestnetLightwalletdPresets,
    ]) {
      expect(list.map((p) => p.url).toSet().length, list.length);
      for (final preset in list) {
        expect(checkEndpointUrl(preset.url).url, preset.url, reason: preset.url);
        expect(preset.url, startsWith('https://'), reason: preset.url);
      }
    }
  });

  test('fallbacks are the other presets in order; none from a custom server', () {
    const list = kMainnetLightwalletdPresets;
    final fromEu = lightwalletdFallbacks('https://eu.zec.rocks:443', list);
    expect(fromEu.length, list.length - 1);
    expect(fromEu.map((p) => p.url), isNot(contains('https://eu.zec.rocks:443')));
    expect(fromEu.first.url, kMainnetLightwalletdUrl);
    expect(lightwalletdFallbacks('https://my.own.server:443', list), isEmpty);
    expect(lightwalletdPresetFor('https://my.own.server:443', list), isNull);
  });

  test('only server-side lightwalletd failures trigger a failover', () {
    bool problem(SyncFailureKind kind, [SyncEndpoint? endpoint]) =>
        ServerFailoverNotifier.serverProblem(
          SyncFailure(kind: kind, endpoint: endpoint),
        );
    const lwd = SyncEndpoint.lightwalletd;
    expect(problem(SyncFailureKind.lightwalletdUnreachable, lwd), isTrue);
    expect(problem(SyncFailureKind.timeout, lwd), isTrue);
    expect(problem(SyncFailureKind.tls, lwd), isTrue);
    expect(problem(SyncFailureKind.serverBehind, lwd), isTrue);
    // The phone is offline, the relay failed, or it's this app's own problem.
    expect(problem(SyncFailureKind.offline), isFalse);
    expect(problem(SyncFailureKind.timeout, SyncEndpoint.relay), isFalse);
    expect(problem(SyncFailureKind.wrongNetwork, lwd), isFalse);
    expect(problem(SyncFailureKind.walletDatabase), isFalse);
  });
}

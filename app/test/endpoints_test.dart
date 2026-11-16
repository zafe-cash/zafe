import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:zafe/src/core/config/endpoints.dart';
import 'package:zafe/src/core/config/network_config.dart';

void main() {
  group('checkEndpointUrl', () {
    String? ok(String url, {String network = 'test'}) =>
        checkEndpointUrl(url, network: network).url;
    String? bad(String url, {String network = 'test'}) =>
        checkEndpointUrl(url, network: network).error;

    test('accepts https and normalizes', () {
      expect(ok('https://relay.example.com'), 'https://relay.example.com');
      expect(ok('  https://Relay.Example.com/  '), 'https://relay.example.com');
      expect(
        ok('https://testnet.zec.rocks:443'),
        'https://testnet.zec.rocks:443',
      );
      expect(ok('HTTPS://a.b:8443'), 'https://a.b:8443');
    });

    test('rejects empty, partial and odd URLs', () {
      expect(bad(''), isNotNull);
      expect(bad('relay.example.com'), isNotNull);
      expect(bad('ftp://relay.example.com'), isNotNull);
      expect(bad('https://relay.example.com/v1'), isNotNull);
      expect(bad('https://relay.example.com?x=1'), isNotNull);
      expect(bad('https://user:pw@relay.example.com'), isNotNull);
    });

    test('plain http only for a local server on regtest', () {
      for (final host in ['localhost', '127.0.0.1', '10.0.2.2']) {
        expect(
          ok('http://$host:8787', network: 'regtest'),
          'http://$host:8787',
        );
        expect(bad('http://$host:8787', network: 'test'), isNotNull);
        expect(bad('http://$host:8787', network: 'main'), isNotNull);
      }
      expect(bad('http://relay.example.com', network: 'regtest'), isNotNull);
      expect(bad('http://192.168.1.5:8787', network: 'regtest'), isNotNull);
      expect(ok('https://localhost:8443'), 'https://localhost:8443');
    });
  });

  group('ZafeEndpoints', () {
    test('defaults are the build defaults', () {
      const d = ZafeEndpoints.defaults;
      expect(d.relayUrl, kZafeRelayUrl);
      expect(d.lightwalletdUrl, kZafeLightwalletdUrl);
      expect(d.isDefault(EndpointKind.relay), isTrue);
      expect(d.relayIsPlaceholder, isFalse);
    });

    test('overrides persist per network and reset to default', () async {
      SharedPreferences.setMockInitialValues({});
      expect(
        (await ZafeEndpoints.load()).lightwalletdUrl,
        kZafeLightwalletdUrl,
      );
      await ZafeEndpoints.save(EndpointKind.relay, 'https://relay.example.com');
      final saved = await ZafeEndpoints.load(reload: true);
      expect(saved.relayUrl, 'https://relay.example.com');
      expect(saved.isDefault(EndpointKind.relay), isFalse);
      expect(saved.lightwalletdUrl, kZafeLightwalletdUrl);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString(kRelayUrlKey), 'https://relay.example.com');
      expect(kRelayUrlKey, endsWith(kZafeNetwork));

      await ZafeEndpoints.save(EndpointKind.relay, kZafeRelayUrl);
      expect(prefs.getString(kRelayUrlKey), isNull);
      expect((await ZafeEndpoints.load()).relayUrl, kZafeRelayUrl);
    });

    test(
      'a saved URL that no longer validates falls back to default',
      () async {
        SharedPreferences.setMockInitialValues({
          kLightwalletdUrlKey: 'not a url',
        });
        final e = await ZafeEndpoints.load();
        expect(e.lightwalletdUrl, kZafeLightwalletdUrl);
      },
    );

    test('withUrl changes one endpoint', () {
      final e = ZafeEndpoints.defaults.withUrl(
        EndpointKind.lightwalletd,
        'https://lwd.example.com',
      );
      expect(e.url(EndpointKind.lightwalletd), 'https://lwd.example.com');
      expect(e.relayUrl, kZafeRelayUrl);
    });
  });
}

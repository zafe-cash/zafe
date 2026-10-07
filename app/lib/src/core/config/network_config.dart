/// Build-time network and endpoint configuration.
///
/// `--dart-define=ZAFE_NETWORK=regtest|test|main` (default `regtest` during development;
/// `testnet` and `mainnet` are accepted as aliases). Each network has a preset relay and
/// lightwalletd URL; `--dart-define=ZAFE_RELAY_URL=...` and
/// `--dart-define=ZAFE_LIGHTWALLETD_URL=...` override them.
///
/// A testnet build for testers:
/// `--dart-define=ZAFE_NETWORK=test --dart-define=ZAFE_RELAY_URL=https://<your relay>`
/// (the preset relay URL is a placeholder until the hosted relay exists).
library;

/// Endpoints for one network. `https` URLs use TLS with the bundled Mozilla roots.
class NetworkPreset {
  const NetworkPreset({
    required this.network,
    required this.relayUrl,
    required this.lightwalletdUrl,
  });

  /// Network name as the Rust core expects it (`main`, `test`, `regtest`).
  final String network;
  final String relayUrl;
  final String lightwalletdUrl;
}

/// Local Zakura regtest and relay (`infra/regtest`, `scripts/app-harness.sh`).
const NetworkPreset kRegtestPreset = NetworkPreset(
  network: 'regtest',
  relayUrl: kRegtestRelayUrl,
  lightwalletdUrl: kRegtestLightwalletdUrl,
);
const String kRegtestRelayUrl = 'http://127.0.0.1:8787';
const String kRegtestLightwalletdUrl = 'http://127.0.0.1:9067';

/// Public testnet: zec.rocks runs an Ironwood-aware lightwalletd. The relay URL is a
/// placeholder (`.invalid` never resolves) until the hosted relay is deployed
/// (`infra/relay/README.md`); pass `ZAFE_RELAY_URL`.
const NetworkPreset kTestnetPreset = NetworkPreset(
  network: 'test',
  relayUrl: kTestnetRelayUrl,
  lightwalletdUrl: kTestnetLightwalletdUrl,
);
const String kTestnetRelayUrl = kPlaceholderRelayUrl;
const String kTestnetLightwalletdUrl = 'https://testnet.zec.rocks:443';

/// Mainnet waits on the audit and a dry run (docs/tracker.md, M2). Its relay is the
/// capped-beta relay on the same VPS as testnet (`infra/relay/README.md`, "Mainnet"):
/// deployed by `.github/workflows/relay-mainnet.yml` once the user has done the one-time
/// setup (DNS, bucket, environment); until then this name doesn't resolve and a mainnet
/// build shows "Can't reach relay".
const NetworkPreset kMainnetPreset = NetworkPreset(
  network: 'main',
  relayUrl: kMainnetRelayUrl,
  lightwalletdUrl: kMainnetLightwalletdUrl,
);
const String kMainnetRelayUrl = 'https://relay.zafe.cash';
const String kMainnetLightwalletdUrl = 'https://zec.rocks:443';

const String kPlaceholderRelayUrl = 'https://relay.zafe.invalid';

const String _rawNetwork = String.fromEnvironment(
  'ZAFE_NETWORK',
  defaultValue: 'regtest',
);

/// The network name passed to Rust: `main`, `test` or `regtest`.
const String kZafeNetwork = _rawNetwork == 'testnet'
    ? 'test'
    : _rawNetwork == 'mainnet'
    ? 'main'
    : _rawNetwork;

/// The preset for [kZafeNetwork] (before any URL override).
const NetworkPreset kZafePreset = kZafeNetwork == 'test'
    ? kTestnetPreset
    : kZafeNetwork == 'main'
    ? kMainnetPreset
    : kRegtestPreset;

// Const expressions can't read fields of const objects, so the defaults are picked here.
const String _presetRelayUrl = kZafeNetwork == 'test'
    ? kTestnetRelayUrl
    : kZafeNetwork == 'main'
    ? kMainnetRelayUrl
    : kRegtestRelayUrl;

const String _presetLightwalletdUrl = kZafeNetwork == 'test'
    ? kTestnetLightwalletdUrl
    : kZafeNetwork == 'main'
    ? kMainnetLightwalletdUrl
    : kRegtestLightwalletdUrl;

const String kZafeRelayUrl = String.fromEnvironment(
  'ZAFE_RELAY_URL',
  defaultValue: _presetRelayUrl,
);

const String kZafeLightwalletdUrl = String.fromEnvironment(
  'ZAFE_LIGHTWALLETD_URL',
  defaultValue: _presetLightwalletdUrl,
);

/// True when this build still points at the placeholder relay (no relay configured).
const bool kZafeRelayIsPlaceholder = kZafeRelayUrl == kPlaceholderRelayUrl;

/// Ticker shown next to amounts: ZEC on mainnet, TAZ on test networks.
const String kZcashDefaultCurrencyTicker = kZafeNetwork == 'main'
    ? 'ZEC'
    : 'TAZ';

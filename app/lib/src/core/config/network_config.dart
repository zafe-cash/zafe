library;

class NetworkPreset {
  const NetworkPreset({
    required this.network,
    required this.relayUrl,
    required this.lightwalletdUrl,
  });

  final String network;
  final String relayUrl;
  final String lightwalletdUrl;
}

const NetworkPreset kRegtestPreset = NetworkPreset(
  network: 'regtest',
  relayUrl: kRegtestRelayUrl,
  lightwalletdUrl: kRegtestLightwalletdUrl,
);
const String kRegtestRelayUrl = 'http://127.0.0.1:8787';
const String kRegtestLightwalletdUrl = 'http://127.0.0.1:9067';

const NetworkPreset kTestnetPreset = NetworkPreset(
  network: 'test',
  relayUrl: kTestnetRelayUrl,
  lightwalletdUrl: kTestnetLightwalletdUrl,
);
const String kTestnetRelayUrl = kPlaceholderRelayUrl;
const String kTestnetLightwalletdUrl = 'https://testnet.zec.rocks:443';

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

const String kZafeNetwork = _rawNetwork == 'testnet'
    ? 'test'
    : _rawNetwork == 'mainnet'
    ? 'main'
    : _rawNetwork;

const NetworkPreset kZafePreset = kZafeNetwork == 'test'
    ? kTestnetPreset
    : kZafeNetwork == 'main'
    ? kMainnetPreset
    : kRegtestPreset;

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

const bool kZafeRelayIsPlaceholder = kZafeRelayUrl == kPlaceholderRelayUrl;

const String kZcashDefaultCurrencyTicker = kZafeNetwork == 'main'
    ? 'ZEC'
    : 'TAZ';

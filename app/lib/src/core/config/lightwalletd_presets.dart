library;

import 'network_config.dart';

class LightwalletdPreset {
  const LightwalletdPreset({
    required this.operator,
    required this.region,
    required this.url,
  });

  final String operator;
  final String region;
  final String url;

  String get label => '$operator · $region';
}

const List<LightwalletdPreset> kMainnetLightwalletdPresets = [
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'Global',
    url: kMainnetLightwalletdUrl,
  ),
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'North America',
    url: 'https://na.zec.rocks:443',
  ),
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'Europe',
    url: 'https://eu.zec.rocks:443',
  ),
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'Asia-Pacific',
    url: 'https://ap.zec.rocks:443',
  ),
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'South America',
    url: 'https://sa.zec.rocks:443',
  ),
  LightwalletdPreset(
    operator: 'Fastz',
    region: 'Global',
    url: 'https://rpc.fastz.cash:443',
  ),
  LightwalletdPreset(
    operator: 'Fastz',
    region: 'North America',
    url: 'https://na.fastz.cash:443',
  ),
  LightwalletdPreset(
    operator: 'Fastz',
    region: 'Europe',
    url: 'https://eu.fastz.cash:443',
  ),
  LightwalletdPreset(
    operator: 'Fastz',
    region: 'Asia-Pacific',
    url: 'https://ap.fastz.cash:443',
  ),
  LightwalletdPreset(
    operator: 'Stardust',
    region: 'United States',
    url: 'https://us.zec.stardust.rest:443',
  ),
  LightwalletdPreset(
    operator: 'Stardust',
    region: 'Europe',
    url: 'https://eu.zec.stardust.rest:443',
  ),
  LightwalletdPreset(
    operator: 'Zcash Explorer',
    region: 'Global',
    url: 'https://lwd.zcashexplorer.app:9067',
  ),
];

const List<LightwalletdPreset> kTestnetLightwalletdPresets = [
  LightwalletdPreset(
    operator: 'Zec Rocks',
    region: 'Testnet',
    url: kTestnetLightwalletdUrl,
  ),
];

const List<LightwalletdPreset> kRegtestLightwalletdPresets = [
  LightwalletdPreset(
    operator: 'Local regtest',
    region: 'This computer',
    url: kRegtestLightwalletdUrl,
  ),
];

const List<LightwalletdPreset> kLightwalletdPresets = kZafeNetwork == 'main'
    ? kMainnetLightwalletdPresets
    : kZafeNetwork == 'test'
    ? kTestnetLightwalletdPresets
    : kRegtestLightwalletdPresets;

LightwalletdPreset? lightwalletdPresetFor(
  String url, [
  List<LightwalletdPreset> presets = kLightwalletdPresets,
]) {
  for (final preset in presets) {
    if (preset.url == url) return preset;
  }
  return null;
}

List<LightwalletdPreset> lightwalletdFallbacks(
  String url, [
  List<LightwalletdPreset> presets = kLightwalletdPresets,
]) => lightwalletdPresetFor(url, presets) == null
    ? const []
    : [
        for (final preset in presets)
          if (preset.url != url) preset,
      ];

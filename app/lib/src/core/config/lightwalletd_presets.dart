/// Public lightwalletd servers offered in Settings for each network, and the fallbacks
/// sync switches to when the chosen one stops answering.
///
/// All of these answered `GetLightdInfo` on the right chain on 2026-10-06 (Stardust's
/// `eu2`/`jp` hosts and two community servers with broken TLS were left out). The first
/// preset of each network is its build default (`network_config.dart`).
library;

import 'network_config.dart';

class LightwalletdPreset {
  const LightwalletdPreset({
    required this.operator,
    required this.region,
    required this.url,
  });

  /// Who runs it, e.g. `Zec Rocks`.
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

/// The presets for this build's network.
const List<LightwalletdPreset> kLightwalletdPresets = kZafeNetwork == 'main'
    ? kMainnetLightwalletdPresets
    : kZafeNetwork == 'test'
    ? kTestnetLightwalletdPresets
    : kRegtestLightwalletdPresets;

/// The preset with this URL, or null for a custom server.
LightwalletdPreset? lightwalletdPresetFor(
  String url, [
  List<LightwalletdPreset> presets = kLightwalletdPresets,
]) {
  for (final preset in presets) {
    if (preset.url == url) return preset;
  }
  return null;
}

/// Where sync may fail over from `url`: the other presets in list order. A custom server
/// gets none, so the app never leaves a server the user chose by hand.
List<LightwalletdPreset> lightwalletdFallbacks(
  String url, [
  List<LightwalletdPreset> presets = kLightwalletdPresets,
]) => lightwalletdPresetFor(url, presets) == null
    ? const []
    : [
        for (final preset in presets)
          if (preset.url != url) preset,
      ];

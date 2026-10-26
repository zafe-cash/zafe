/// Relays offered by the relay picker (Create > More options > Relay, Settings), per
/// network. Community relays are added here by pull request, each with its operator's
/// name; any other address can be typed or scanned. The picker checks only the relay the
/// user selects (`check_relay`), never the whole list: each listed operator would learn
/// this phone's address (docs/relay-move.md §8 B).
library;

import 'docs_links.dart';
import 'endpoints.dart';
import 'network_config.dart';

class RelayPreset {
  const RelayPreset({required this.operator, required this.url});

  /// Who runs it, e.g. `Zafe`.
  final String operator;
  final String url;
}

/// The site page on running a server (Compose, Caddy, backups, the create token).
const kRunYourOwnRelayUrl = kDocsRunAServerUrl;

const List<RelayPreset> kMainnetRelayPresets = [
  RelayPreset(operator: 'Zafe', url: kMainnetRelayUrl),
];

/// The hosted testnet relay (also the release builds' `ZAFE_RELAY_URL`).
const List<RelayPreset> kTestnetRelayPresets = [
  RelayPreset(operator: 'Zafe', url: 'https://testnet.relay.zafe.cash'),
];

const List<RelayPreset> kRegtestRelayPresets = [
  RelayPreset(operator: 'This computer', url: kRegtestRelayUrl),
];

const List<RelayPreset> _networkRelayPresets = kZafeNetwork == 'main'
    ? kMainnetRelayPresets
    : kZafeNetwork == 'test'
    ? kTestnetRelayPresets
    : kRegtestRelayPresets;

/// The presets for this build's network, with the build's own relay first when a
/// `ZAFE_RELAY_URL` override names one that isn't listed.
List<RelayPreset> get kRelayPresets => [
  if (!kZafeRelayIsPlaceholder &&
      !_networkRelayPresets.any((p) => p.url == kZafeRelayUrl))
    const RelayPreset(operator: 'Zafe', url: kZafeRelayUrl),
  ..._networkRelayPresets,
];

/// The listed relay at `url`, or null for another address.
RelayPreset? relayPresetFor(String url, [List<RelayPreset>? presets]) {
  for (final preset in presets ?? kRelayPresets) {
    if (preset.url == url) return preset;
  }
  return null;
}

/// A relay address the user typed, pasted or scanned: `relay.example.org`,
/// `https://relay.example.org`, or either with `#<create token>` (how a private relay's
/// operator hands both out at once).
class RelayInput {
  const RelayInput.ok(String this.url, {this.token}) : error = null;
  const RelayInput.invalid(String this.error) : url = null, token = null;

  final String? url;
  final String? token;
  final String? error;
}

RelayInput parseRelayInput(String input, {String network = kZafeNetwork}) {
  var text = input.trim();
  String? token;
  final hash = text.indexOf('#');
  if (hash >= 0) {
    token = text.substring(hash + 1).trim();
    text = text.substring(0, hash).trim();
    if (token.isEmpty) token = null;
  }
  if (text.isNotEmpty && !text.contains('://')) text = 'https://$text';
  final check = checkEndpointUrl(text, network: network);
  if (check.url == null) return RelayInput.invalid(check.error!);
  return RelayInput.ok(check.url!, token: token);
}

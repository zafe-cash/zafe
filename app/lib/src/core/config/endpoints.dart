library;

import 'package:shared_preferences/shared_preferences.dart';

import 'network_config.dart';

enum EndpointKind { relay, lightwalletd }

const kRelayUrlKey = 'zafe_relay_url_$kZafeNetwork';
const kLightwalletdUrlKey = 'zafe_lightwalletd_url_$kZafeNetwork';

class ZafeEndpoints {
  const ZafeEndpoints({
    this.relayUrl = kZafeRelayUrl,
    this.lightwalletdUrl = kZafeLightwalletdUrl,
  });

  final String relayUrl;
  final String lightwalletdUrl;

  static const defaults = ZafeEndpoints();

  String url(EndpointKind kind) => switch (kind) {
    EndpointKind.relay => relayUrl,
    EndpointKind.lightwalletd => lightwalletdUrl,
  };

  bool isDefault(EndpointKind kind) => url(kind) == defaults.url(kind);

  bool get relayIsPlaceholder => relayUrl == kPlaceholderRelayUrl;

  ZafeEndpoints withUrl(EndpointKind kind, String url) => switch (kind) {
    EndpointKind.relay => ZafeEndpoints(
      relayUrl: url,
      lightwalletdUrl: lightwalletdUrl,
    ),
    EndpointKind.lightwalletd => ZafeEndpoints(
      relayUrl: relayUrl,
      lightwalletdUrl: url,
    ),
  };

  static Future<ZafeEndpoints> load({bool reload = false}) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      if (reload) await prefs.reload();
      return fromPrefs(prefs);
    } catch (_) {
      return defaults;
    }
  }

  static ZafeEndpoints fromPrefs(SharedPreferences prefs) {
    String pick(String key, EndpointKind kind) {
      final saved = prefs.getString(key);
      if (saved == null) return defaults.url(kind);
      final check = checkEndpointUrl(saved);
      return check.url ?? defaults.url(kind);
    }

    return ZafeEndpoints(
      relayUrl: pick(kRelayUrlKey, EndpointKind.relay),
      lightwalletdUrl: pick(kLightwalletdUrlKey, EndpointKind.lightwalletd),
    );
  }

  static Future<void> save(EndpointKind kind, String url) async {
    final prefs = await SharedPreferences.getInstance();
    final key = kind == EndpointKind.relay ? kRelayUrlKey : kLightwalletdUrlKey;
    if (url == defaults.url(kind)) {
      await prefs.remove(key);
    } else {
      await prefs.setString(key, url);
    }
  }
}

class EndpointUrlCheck {
  const EndpointUrlCheck.ok(String this.url) : error = null;
  const EndpointUrlCheck.invalid(String this.error) : url = null;
  final String? url;
  final String? error;
}

const kLocalEndpointHosts = {'localhost', '127.0.0.1', '10.0.2.2'};

EndpointUrlCheck checkEndpointUrl(
  String input, {
  String network = kZafeNetwork,
}) {
  final text = input.trim();
  if (text.isEmpty) return const EndpointUrlCheck.invalid('Enter a URL.');
  final uri = Uri.tryParse(text);
  if (uri == null || !uri.hasScheme || uri.host.isEmpty) {
    return const EndpointUrlCheck.invalid(
      'Enter a full URL, like https://example.com:443.',
    );
  }
  final scheme = uri.scheme.toLowerCase();
  if (scheme != 'https' && scheme != 'http') {
    return const EndpointUrlCheck.invalid('Use an https:// URL.');
  }
  if (uri.userInfo.isNotEmpty ||
      (uri.path.isNotEmpty && uri.path != '/') ||
      uri.hasQuery ||
      uri.hasFragment) {
    return const EndpointUrlCheck.invalid(
      'Use only the server address and port, without a path.',
    );
  }
  final host = uri.host.toLowerCase();
  if (scheme == 'http' &&
      !(network == 'regtest' && kLocalEndpointHosts.contains(host))) {
    return const EndpointUrlCheck.invalid(
      'Use an https:// URL. Plain http is only allowed for a local regtest server.',
    );
  }
  final explicitPort = RegExp(
    r':(\d+)$',
  ).firstMatch(RegExp(r'^[^:]+://([^/?#]*)').firstMatch(text)?.group(1) ?? '');
  final port = uri.hasPort
      ? ':${uri.port}'
      : (explicitPort != null ? ':${explicitPort.group(1)}' : '');
  final hostPart = host.contains(':') ? '[$host]' : host;
  return EndpointUrlCheck.ok('$scheme://$hostPart$port');
}

/// Invite links, the tappable and scannable form of a raw invite (`zafe-invite-v<n>:<hex>`, n = `version::INVITE`):
///
/// - `https://<ZAFE_LINK_HOST>/join#<invite>` when the build has a link host. It opens Zafe
///   when installed (Android App Links, iOS Universal Links) and the landing page otherwise
///   (`infra/site/`). The invite is in the fragment, so the web server never sees it.
/// - `zafe://join?invite=<invite>` otherwise, and always accepted.
///
/// Pure string handling; the invite itself is validated by Rust (`parseInvite`) wherever
/// it's used.
library;

import '../../core/config/network_config.dart';

/// The scheme this build registers: the testnet app has its own, so a link opens the app
/// it was made for when both are installed. Both builds still read the other's links.
const kInviteLinkScheme = kZafeNetwork == 'test' ? 'zafe-testnet' : 'zafe';
const _kInviteLinkSchemes = {'zafe', 'zafe-testnet'};
const kInviteLinkHost = 'join';

/// The web host of `https` invite links (`--dart-define=ZAFE_LINK_HOST=zafe.example`),
/// empty when the build has none. Leave it unset until the landing site is deployed on a
/// domain we control: whoever serves that host's page can read the invite in the fragment.
const kZafeLinkHost = String.fromEnvironment('ZAFE_LINK_HOST');

/// Path of `https` invite links on [kZafeLinkHost].
const kInviteLinkPath = '/join';

/// Every raw invite starts with this (the version follows it).
const _rawInvitePrefix = 'zafe-invite-';

/// Characters that may appear unescaped in a query value or fragment (RFC 3986
/// unreserved plus ':').
final _urlSafe = RegExp(r'^[A-Za-z0-9._~:-]+$');

/// The link that opens Zafe's Join screen with [invite] filled in: the `https` form on
/// [linkHost], or the `zafe://` form when there is none. Today's invites (prefix + hex)
/// are URL-safe and embedded as they are.
String inviteLink(String invite, {String linkHost = kZafeLinkHost}) {
  final raw = invite.trim();
  if (linkHost.isEmpty) {
    final value = _urlSafe.hasMatch(raw) ? raw : Uri.encodeQueryComponent(raw);
    return '$kInviteLinkScheme://$kInviteLinkHost?invite=$value';
  }
  final value = _urlSafe.hasMatch(raw) ? raw : Uri.encodeComponent(raw);
  return 'https://${linkHost.toLowerCase()}$kInviteLinkPath#$value';
}

/// True when [uri] is an invite link (either form), whether or not it carries a valid
/// invite: such a link is shown on the Join screen, which says it isn't an invite.
bool isInviteLink(Uri uri, {String linkHost = kZafeLinkHost}) {
  final scheme = uri.scheme.toLowerCase();
  if (_kInviteLinkSchemes.contains(scheme)) return true;
  return scheme == 'https' &&
      linkHost.isNotEmpty &&
      uri.host.toLowerCase() == linkHost.toLowerCase();
}

/// The raw invite carried by [uri], or null when it isn't a Zafe invite link.
String? inviteFromLink(Uri uri, {String linkHost = kZafeLinkHost}) {
  final scheme = uri.scheme.toLowerCase();
  if (_kInviteLinkSchemes.contains(scheme)) return _fromSchemeLink(uri);
  if (scheme != 'https' || linkHost.isEmpty) return null;
  return _fromWebLink(uri, linkHost);
}

String? _fromSchemeLink(Uri uri) {
  if (uri.host.toLowerCase() != kInviteLinkHost) return null;
  if (uri.path.isNotEmpty && uri.path != '/') return null;
  final List<String> values;
  try {
    values = uri.queryParametersAll['invite'] ?? const [];
  } on FormatException {
    return null; // bad percent-encoding
  }
  if (values.length != 1) return null;
  return _rawInvite(values.single);
}

/// `https://<host>/join#<invite>`: exactly that host (default port, no user info), the
/// join path, no query, the invite as the whole fragment.
String? _fromWebLink(Uri uri, String linkHost) {
  if (uri.host.toLowerCase() != linkHost.toLowerCase()) return null;
  if (uri.userInfo.isNotEmpty || (uri.hasPort && uri.port != 443)) return null;
  const paths = [kInviteLinkPath, '$kInviteLinkPath/'];
  if (!paths.contains(uri.path)) return null;
  if (uri.hasQuery || !uri.hasFragment) return null;
  final String value;
  try {
    value = Uri.decodeComponent(uri.fragment);
  } on FormatException {
    return null; // bad percent-encoding
  } on ArgumentError {
    return null; // the same, on older SDKs
  }
  return _rawInvite(value);
}

String? _rawInvite(String value) {
  final invite = value.trim();
  return invite.startsWith(_rawInvitePrefix) ? invite : null;
}

/// The raw invite in [text] (pasted, scanned or typed): a raw invite or an invite link,
/// alone or inside a shared message. Null when there's none. The result still needs
/// `parseInvite`.
String? extractInvite(String text, {String linkHost = kZafeLinkHost}) {
  for (final token in text.trim().split(RegExp(r'\s+'))) {
    final invite = _inviteIn(token, linkHost);
    if (invite != null) return invite;
  }
  return null;
}

String? _inviteIn(String token, String linkHost) {
  if (token.startsWith(_rawInvitePrefix)) return token;
  final lower = token.toLowerCase();
  final isSchemeLink = _kInviteLinkSchemes.any((s) => lower.startsWith('$s:'));
  if (!isSchemeLink && !lower.startsWith('https:')) {
    return null;
  }
  final uri = Uri.tryParse(token);
  return uri == null ? null : inviteFromLink(uri, linkHost: linkHost);
}

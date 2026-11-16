library;

import '../../core/config/network_config.dart';

const kInviteLinkScheme = kZafeNetwork == 'test' ? 'zafe-testnet' : 'zafe';
const _kInviteLinkSchemes = {'zafe', 'zafe-testnet'};
const kInviteLinkHost = 'join';

const kZafeLinkHost = String.fromEnvironment('ZAFE_LINK_HOST');

const kInviteLinkPath = '/join';

const _rawInvitePrefix = 'zafe-invite-';

final _urlSafe = RegExp(r'^[A-Za-z0-9._~:-]+$');

String inviteLink(String invite, {String linkHost = kZafeLinkHost}) {
  final raw = invite.trim();
  if (linkHost.isEmpty) {
    final value = _urlSafe.hasMatch(raw) ? raw : Uri.encodeQueryComponent(raw);
    return '$kInviteLinkScheme://$kInviteLinkHost?invite=$value';
  }
  final value = _urlSafe.hasMatch(raw) ? raw : Uri.encodeComponent(raw);
  return 'https://${linkHost.toLowerCase()}$kInviteLinkPath#$value';
}

bool isInviteLink(Uri uri, {String linkHost = kZafeLinkHost}) {
  final scheme = uri.scheme.toLowerCase();
  if (_kInviteLinkSchemes.contains(scheme)) return true;
  return scheme == 'https' &&
      linkHost.isNotEmpty &&
      uri.host.toLowerCase() == linkHost.toLowerCase();
}

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
    return null;
  }
  if (values.length != 1) return null;
  return _rawInvite(values.single);
}

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
    return null;
  } on ArgumentError {
    return null;
  }
  return _rawInvite(value);
}

String? _rawInvite(String value) {
  final invite = value.trim();
  return invite.startsWith(_rawInvitePrefix) ? invite : null;
}

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

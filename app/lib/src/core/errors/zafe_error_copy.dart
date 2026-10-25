import '../../rust/api/error.dart';
import '../config/network_config.dart';

/// Friendly copy for errors from Rust. Switches on the typed `ZafeErrorKind` (no
/// substring matching); `fallback` covers `other`.
/// A failure the app explains itself (not from Rust): its text is shown as is.
class UserFacingError implements Exception {
  const UserFacingError(this.message);
  final String message;

  @override
  String toString() => message;
}

String zafeErrorMessage(
  Object error, {
  String fallback = 'Something went wrong. Try again.',
}) {
  if (error is UserFacingError) return error.message;
  if (error is! ZafeError) return fallback;
  return switch (error.kind) {
    ZafeErrorKind.network =>
      error.endpoint == ZafeEndpoint.none
          ? 'Network error. Check your connection and try again.'
          : 'Can\'t reach the ${_server(error.endpoint)}. Check your connection '
                'and try again.',
    ZafeErrorKind.notReady => _sentence(error.message),
    ZafeErrorKind.keygenIncomplete =>
      'Key generation stopped: some members never took part. Ask them to open Zafe, '
          'then try again together.',
    ZafeErrorKind.inviteExpired =>
      'This invite expired. Ask the vault\'s creator to share it again.',
    ZafeErrorKind.timeout =>
      'Other signers haven\'t answered yet. Ask them to open Zafe, then try again.',
    ZafeErrorKind.verification =>
      'This payment failed the check on your device. Don\'t approve it. (${error.message})',
    ZafeErrorKind.insufficientFunds =>
      'Not enough $kZcashDefaultCurrencyTicker in the vault to cover the amount and the fee.',
    ZafeErrorKind.fundsReserved =>
      'Some funds are held by open payments. Wait for one to finish, or send less.',
    ZafeErrorKind.invalidInput => _sentence(error.message),
    ZafeErrorKind.updateRequired =>
      'This needs a newer version of Zafe. Update the app, then try again.',
    ZafeErrorKind.relayOutdated =>
      'The relay server runs an older version of Zafe than this app. '
          'Ask whoever runs it to update it.',
    ZafeErrorKind.createTokenRefused =>
      'This relay is private: it needs its create token. Ask whoever runs it.',
    ZafeErrorKind.relayStorageFull =>
      'The relay\'s storage for this vault is full. Old messages expire after 30 days; if it keeps happening, ask the relay\'s operator for more space.',
    ZafeErrorKind.relayRolledBack =>
      'The relay lost part of this vault\'s history, so Zafe ignores it. A member with the full history can restore it from the sync status on Home.',
    ZafeErrorKind.relayLostVault =>
      'The relay no longer has this vault. A member can restore it from the sync '
          'status on Home.',
    ZafeErrorKind.relayForked =>
      'The relay shows a different history than this phone saw, so Zafe ignores it. Check with the other members first.',
    ZafeErrorKind.logCopyInvalid =>
      'This phone\'s saved history is damaged. Check with the other members, then follow the relay.',
    ZafeErrorKind.relayMembership =>
      'The relay\'s members or approval rule don\'t match this vault, so Zafe ignores it. Your funds are safe. Agree on a new relay with the others.',
    ZafeErrorKind.tls =>
      'Couldn\'t connect securely to the ${_server(error.endpoint)}. '
          'Check its address in server settings.',
    ZafeErrorKind.networkTimeout =>
      'The ${_server(error.endpoint)} didn\'t answer in time. Try again.',
    ZafeErrorKind.serverBehind =>
      'The Zcash server is behind this wallet. Wait a moment, or pick another '
          'one in server settings.',
    ZafeErrorKind.wrongNetwork =>
      'The Zcash server is on another network. Pick another one in server settings.',
    ZafeErrorKind.walletDatabase =>
      'This phone couldn\'t read the vault\'s wallet data. Try again.',
    ZafeErrorKind.torConnecting =>
      'Zafe is still connecting to Tor. Nothing was sent; try again in a moment.',
    ZafeErrorKind.torFailed =>
      'Tor couldn\'t connect, so nothing was sent. Try again, or turn off Tor in '
          'Settings.',
    ZafeErrorKind.vaultNotOnRelay =>
      'This vault isn\'t on this relay. Check the relay address in server settings; all members must use the same one.',
    ZafeErrorKind.vaultMoved =>
      'The vault moved to ${_host(error.message)}. Move to continue.',
    ZafeErrorKind.moveInProgress =>
      'You\'re already moving this vault. Cancel that move first.',
    ZafeErrorKind.membersMustUpdate =>
      error.members.length == 1
          ? 'A member must update Zafe before the vault can move.'
          : '${error.members.length} members must update Zafe before the vault can move.',
    ZafeErrorKind.moveNotFound =>
      'No member has moved to that relay with this link. Ask for a new one.',
    ZafeErrorKind.moveExpired => 'This move expired. Ask for a new link.',
    ZafeErrorKind.moveStale =>
      'This link is from an earlier move. Ask for a new one.',
    ZafeErrorKind.twoMoves => 'Two moves found: ask your group.',
    ZafeErrorKind.other => fallback,
  };
}

String _host(String url) => Uri.tryParse(url)?.authority ?? url;

String _server(ZafeEndpoint endpoint) => switch (endpoint) {
  ZafeEndpoint.relay => 'relay',
  ZafeEndpoint.lightwalletd => 'Zcash server',
  ZafeEndpoint.none => 'server',
};

String _sentence(String message) {
  final m = message.replaceFirst(RegExp(r'^membership is not ready: '), '');
  if (m.isEmpty) return m;
  final s = m[0].toUpperCase() + m.substring(1);
  return s.endsWith('.') ? s : '$s.';
}

/// For logs: the generated `ZafeError` has no useful `toString`.
String describeError(Object error) =>
    error is ZafeError ? '${error.kind.name}: ${error.message}' : '$error';

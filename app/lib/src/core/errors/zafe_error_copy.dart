import '../../rust/api/error.dart';
import '../config/network_config.dart';

/// Friendly copy for errors from Rust. Switches on the typed `ZafeErrorKind` (no
/// substring matching); `fallback` covers `other`.
String zafeErrorMessage(
  Object error, {
  String fallback = 'Something went wrong. Try again.',
}) {
  if (error is! ZafeError) return fallback;
  return switch (error.kind) {
    ZafeErrorKind.network =>
      error.endpoint == ZafeEndpoint.none
          ? 'Network error. Check your connection and try again.'
          : 'Can\'t reach the ${_server(error.endpoint)}. Check your connection '
                'and try again.',
    ZafeErrorKind.notReady => _sentence(error.message),
    ZafeErrorKind.timeout =>
      'Other signers haven\'t answered yet. Ask them to open Zafe, then try again.',
    ZafeErrorKind.verification =>
      'This payment failed the check on your device. Don\'t approve it. (${error.message})',
    ZafeErrorKind.insufficientFunds =>
      'Not enough $kZcashDefaultCurrencyTicker in the vault to cover the amount and the fee.',
    ZafeErrorKind.fundsReserved =>
      'Part of the vault\'s balance is held by payments that are still open. '
          'Wait until one is sent or cancelled, or propose a smaller amount.',
    ZafeErrorKind.invalidInput => _sentence(error.message),
    ZafeErrorKind.updateRequired =>
      'This needs a newer version of Zafe. Update the app, then try again.',
    ZafeErrorKind.relayOutdated =>
      'The relay server runs an older version of Zafe than this app. '
          'Ask whoever runs it to update it.',
    ZafeErrorKind.relayStorageFull =>
      'The relay\'s storage for this vault is full, so it can\'t take new messages '
          'right now. Old messages expire after 30 days; if this keeps happening, ask '
          'whoever runs the relay to raise the vault\'s limit.',
    ZafeErrorKind.relayAtCapacity =>
      'The Zafe beta is full for now, so this relay takes no new vaults. Vaults '
          'that already exist keep working. Try again later.',
    ZafeErrorKind.relayRolledBack =>
      'The relay lost part of this vault\'s history, so Zafe isn\'t using it. A '
          'member whose phone has the full history can restore it from the sync '
          'status on Home.',
    ZafeErrorKind.relayLostVault =>
      'The relay no longer has this vault. A member can restore it from the sync '
          'status on Home.',
    ZafeErrorKind.relayForked =>
      'The relay shows a different history than this phone saw, so Zafe isn\'t '
          'using it. Check with the other members before doing anything.',
    ZafeErrorKind.tls =>
      'Couldn\'t connect securely to the ${_server(error.endpoint)}. '
          'Check its address in Settings.',
    ZafeErrorKind.networkTimeout =>
      'The ${_server(error.endpoint)} didn\'t answer in time. Try again.',
    ZafeErrorKind.serverBehind =>
      'The Zcash server is behind this wallet. Wait a moment, or pick another '
          'server in Settings.',
    ZafeErrorKind.wrongNetwork =>
      'The Zcash server is on another network. Pick another server in Settings.',
    ZafeErrorKind.walletDatabase =>
      'This phone couldn\'t read the vault\'s wallet data. Try again.',
    ZafeErrorKind.torConnecting =>
      'Zafe is still connecting to Tor. Nothing was sent; try again in a moment.',
    ZafeErrorKind.torFailed =>
      'Tor couldn\'t connect, so nothing was sent. Try again, or turn off Tor in '
          'Settings.',
    ZafeErrorKind.vaultNotOnRelay =>
      'This vault isn\'t on this relay. Check the relay address in Settings, and that '
          'every member uses the same relay.',
    ZafeErrorKind.other => fallback,
  };
}

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

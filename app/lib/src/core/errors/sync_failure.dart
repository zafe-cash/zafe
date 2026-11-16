library;

import '../../rust/api/error.dart';

enum SyncFailureKind {
  offline,
  lightwalletdUnreachable,
  relayUnreachable,

  tls,

  timeout,

  serverBehind,

  wrongNetwork,

  walletDatabase,

  updateRequired,

  relayOutdated,

  relayRolledBack,

  relayLostVault,

  relayForked,

  logCopyInvalid,

  relayMembership,

  torConnecting,

  torFailed,
  other,
}

enum SyncEndpoint { relay, lightwalletd }

class SyncFailure {
  const SyncFailure({required this.kind, this.endpoint, this.detail = ''});

  final SyncFailureKind kind;

  final SyncEndpoint? endpoint;

  final String detail;

  String get statusLabel => switch (kind) {
    SyncFailureKind.offline => 'Offline',
    SyncFailureKind.lightwalletdUnreachable => 'Can\'t reach Zcash server',
    SyncFailureKind.relayUnreachable => 'Can\'t reach server',
    SyncFailureKind.tls => 'Connection not secure',
    SyncFailureKind.timeout => 'Server not responding',
    SyncFailureKind.serverBehind => 'Server behind',
    SyncFailureKind.wrongNetwork => 'Wrong network',
    SyncFailureKind.walletDatabase => 'Storage problem',
    SyncFailureKind.updateRequired => 'Update needed',
    SyncFailureKind.relayOutdated => 'Server outdated',
    SyncFailureKind.relayRolledBack => 'Server lost data',
    SyncFailureKind.relayLostVault => 'Server lost the vault',
    SyncFailureKind.relayForked => 'Server not trusted',
    SyncFailureKind.logCopyInvalid => 'History damaged',
    SyncFailureKind.relayMembership => 'Server not trusted',
    SyncFailureKind.torConnecting => 'Connecting to Tor...',
    SyncFailureKind.torFailed => 'Tor couldn\'t connect',
    SyncFailureKind.other => 'Sync failed',
  };

  bool get isTransient => kind == SyncFailureKind.torConnecting;

  String get title => switch (kind) {
    SyncFailureKind.offline => 'You seem to be offline',
    SyncFailureKind.lightwalletdUnreachable => 'Can\'t reach the Zcash server',
    SyncFailureKind.relayUnreachable => 'Can\'t reach the server',
    SyncFailureKind.tls => 'Secure connection failed',
    SyncFailureKind.timeout => 'The server isn\'t responding',
    SyncFailureKind.serverBehind => 'The Zcash server is behind',
    SyncFailureKind.wrongNetwork => 'The Zcash server is on another network',
    SyncFailureKind.walletDatabase => 'Wallet storage problem',
    SyncFailureKind.updateRequired => 'Update Zafe',
    SyncFailureKind.relayOutdated => 'The server needs an update',
    SyncFailureKind.relayRolledBack => 'The server lost part of the vault',
    SyncFailureKind.relayLostVault => 'The server lost the vault',
    SyncFailureKind.relayForked => 'The server shows another history',
    SyncFailureKind.logCopyInvalid => 'This phone\'s saved history is damaged',
    SyncFailureKind.relayMembership => 'The server lists other members',
    SyncFailureKind.torConnecting => 'Connecting to Tor',
    SyncFailureKind.torFailed => 'Tor couldn\'t connect',
    SyncFailureKind.other => 'Sync failed',
  };

  String get explanation => switch (kind) {
    SyncFailureKind.offline =>
      'Neither the Zcash server nor the vault\'s server answers. Check this phone\'s '
          'internet connection. Zafe keeps trying.',
    SyncFailureKind.lightwalletdUnreachable =>
      'Can\'t reach the Zcash server. Zafe keeps trying; if it lasts, check its address in Settings.',
    SyncFailureKind.relayUnreachable =>
      'Payments can\'t be approved until the server is back. Your funds are safe.',
    SyncFailureKind.tls =>
      'The server\'s security certificate isn\'t valid. Check its address in Settings.',
    SyncFailureKind.timeout =>
      'The server is too slow to answer. Zafe keeps trying.',
    SyncFailureKind.serverBehind =>
      'The Zcash server is behind this wallet. Wait, or pick another server in Settings.',
    SyncFailureKind.wrongNetwork =>
      'The Zcash server serves a different network than this vault. Pick a server '
          'for the right network in Settings.',
    SyncFailureKind.walletDatabase =>
      'This phone couldn\'t read the vault\'s wallet data. Your keys and funds are safe; the data can be rebuilt.',
    SyncFailureKind.updateRequired =>
      'The server or another member uses a newer version of Zafe. Update the app.',
    SyncFailureKind.relayOutdated =>
      'The server runs an older version of Zafe than this app. Ask whoever runs it '
          'to update it.',
    SyncFailureKind.relayRolledBack =>
      'The server lost part of the vault\'s history, so Zafe ignores it. A member with the full history can restore it.',
    SyncFailureKind.relayLostVault =>
      'The server lost this vault. Your funds are safe: any member can restore it from their phone.',
    SyncFailureKind.relayForked =>
      'The server shows a different history than this phone saw, so Zafe won\'t use it. Check with the other members. If they agree the server is right, you can follow it.',
    SyncFailureKind.logCopyInvalid =>
      'This phone\'s saved history is damaged. Your funds are safe. If another member\'s app works normally, follow the server to replace it.',
    SyncFailureKind.relayMembership =>
      'The server\'s members or approval rule don\'t match this vault, so Zafe won\'t use it. Your funds are safe. Agree on a new server with the other members.',
    SyncFailureKind.torConnecting =>
      'Connecting to Tor. Nothing is sent until it connects.',
    SyncFailureKind.torFailed =>
      'Tor couldn\'t connect, so nothing is sent. Try again, or turn off Tor in Settings.',
    SyncFailureKind.other =>
      'Something went wrong while updating the vault. Zafe keeps trying.',
  };

  bool get canRestoreRelay =>
      kind == SyncFailureKind.relayRolledBack ||
      kind == SyncFailureKind.relayLostVault;

  bool get canFollowRelay =>
      kind == SyncFailureKind.relayForked ||
      kind == SyncFailureKind.logCopyInvalid;

  bool get canMoveRelay => switch (kind) {
    SyncFailureKind.relayUnreachable ||
    SyncFailureKind.relayRolledBack ||
    SyncFailureKind.relayLostVault ||
    SyncFailureKind.relayForked ||
    SyncFailureKind.relayMembership ||
    SyncFailureKind.relayOutdated => true,
    _ => false,
  };

  bool get suggestsSettings => switch (kind) {
    SyncFailureKind.lightwalletdUnreachable ||
    SyncFailureKind.relayUnreachable ||
    SyncFailureKind.tls ||
    SyncFailureKind.serverBehind ||
    SyncFailureKind.wrongNetwork ||
    SyncFailureKind.relayMembership ||
    SyncFailureKind.torFailed => true,
    _ => false,
  };
}

SyncEndpoint? _endpoint(ZafeEndpoint e) => switch (e) {
  ZafeEndpoint.relay => SyncEndpoint.relay,
  ZafeEndpoint.lightwalletd => SyncEndpoint.lightwalletd,
  ZafeEndpoint.none => null,
};

SyncFailure classifySyncFailure(Object error, {SyncEndpoint? fallback}) {
  if (error is! ZafeError) {
    return SyncFailure(
      kind: SyncFailureKind.other,
      endpoint: fallback,
      detail: '$error',
    );
  }
  final endpoint = _endpoint(error.endpoint) ?? fallback;
  final kind = switch (error.kind) {
    ZafeErrorKind.network => switch (endpoint) {
      SyncEndpoint.relay => SyncFailureKind.relayUnreachable,
      SyncEndpoint.lightwalletd => SyncFailureKind.lightwalletdUnreachable,
      null => SyncFailureKind.offline,
    },
    ZafeErrorKind.tls => SyncFailureKind.tls,
    ZafeErrorKind.networkTimeout => SyncFailureKind.timeout,
    ZafeErrorKind.serverBehind => SyncFailureKind.serverBehind,
    ZafeErrorKind.wrongNetwork => SyncFailureKind.wrongNetwork,
    ZafeErrorKind.walletDatabase => SyncFailureKind.walletDatabase,
    ZafeErrorKind.updateRequired => SyncFailureKind.updateRequired,
    ZafeErrorKind.relayOutdated => SyncFailureKind.relayOutdated,
    ZafeErrorKind.relayRolledBack => SyncFailureKind.relayRolledBack,
    ZafeErrorKind.relayLostVault => SyncFailureKind.relayLostVault,
    ZafeErrorKind.relayForked => SyncFailureKind.relayForked,
    ZafeErrorKind.logCopyInvalid => SyncFailureKind.logCopyInvalid,
    ZafeErrorKind.relayMembership => SyncFailureKind.relayMembership,
    ZafeErrorKind.torConnecting => SyncFailureKind.torConnecting,
    ZafeErrorKind.torFailed => SyncFailureKind.torFailed,
    _ => SyncFailureKind.other,
  };
  final keepsEndpoint = switch (kind) {
    SyncFailureKind.offline ||
    SyncFailureKind.walletDatabase ||
    SyncFailureKind.updateRequired ||
    SyncFailureKind.torConnecting ||
    SyncFailureKind.torFailed => false,
    _ => true,
  };
  return SyncFailure(
    kind: kind,
    endpoint: keepsEndpoint ? endpoint : null,
    detail: error.message,
  );
}

SyncFailure? homeSyncFailure({Object? syncError, Object? relayError}) {
  final sync = syncError == null
      ? null
      : classifySyncFailure(syncError, fallback: SyncEndpoint.lightwalletd);
  final relay = relayError == null
      ? null
      : classifySyncFailure(relayError, fallback: SyncEndpoint.relay);
  for (final kind in const [
    SyncFailureKind.torFailed,
    SyncFailureKind.torConnecting,
  ]) {
    if (sync?.kind == kind) return sync;
    if (relay?.kind == kind) return relay;
  }
  if (sync?.kind == SyncFailureKind.lightwalletdUnreachable &&
      relay?.kind == SyncFailureKind.relayUnreachable) {
    return SyncFailure(
      kind: SyncFailureKind.offline,
      detail: '${sync!.detail}\n${relay!.detail}',
    );
  }
  return sync ?? relay;
}

const _months = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

String formatLastSuccess(DateTime? at, {DateTime? now}) {
  if (at == null) return 'Not yet';
  final ago = (now ?? DateTime.now()).difference(at);
  if (ago.inMinutes < 1) return 'Just now';
  if (ago.inHours < 1) return '${ago.inMinutes} min ago';
  if (ago.inHours < 24) return '${ago.inHours} h ago';
  final hh = at.hour.toString().padLeft(2, '0');
  final mm = at.minute.toString().padLeft(2, '0');
  return '${at.day} ${_months[at.month - 1]}, $hh:$mm';
}

String formatSince(DateTime at, {DateTime? now}) {
  final today = now ?? DateTime.now();
  if (at.year == today.year && at.month == today.month && at.day == today.day) {
    final hh = at.hour.toString().padLeft(2, '0');
    final mm = at.minute.toString().padLeft(2, '0');
    return '$hh:$mm';
  }
  return '${at.day} ${_months[at.month - 1]}';
}

DateTime? fromUnixSeconds(BigInt? secs) => secs == null
    ? null
    : DateTime.fromMillisecondsSinceEpoch(secs.toInt() * 1000);

const followRelayTitle = 'Follow the server\'s history?';

const followRelayWarning =
    'This phone replaces its saved history with the server\'s. Anything only this phone saw is lost. Your keys and funds are not touched. Do it only once the other members confirm the server is right.';

String followRelayDone(int dropped) => dropped == 0
    ? 'This phone now follows the server.'
    : 'This phone now follows the server. $dropped entries only it had were dropped.';

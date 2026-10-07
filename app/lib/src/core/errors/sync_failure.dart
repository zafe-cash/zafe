/// Why the vault can't stay up to date, from the typed errors of the last wallet sync
/// (lightwalletd) and the last vault refresh (relay). Pure: unit-tested in
/// `test/sync_failure_test.dart`.
library;

import '../../rust/api/error.dart';

enum SyncFailureKind {
  /// Neither server answers: most likely this phone is offline.
  offline,
  lightwalletdUnreachable,
  relayUnreachable,

  /// The secure connection failed (certificate or TLS setup).
  tls,

  /// A server accepted the connection but didn't answer in time.
  timeout,

  /// lightwalletd is behind blocks this wallet already has.
  serverBehind,

  /// lightwalletd serves another network (e.g. mainnet in a testnet build).
  wrongNetwork,

  /// This phone's wallet database failed.
  walletDatabase,

  /// The relay or another member uses a newer Zafe.
  updateRequired,

  /// The relay runs an older Zafe than this app.
  relayOutdated,

  /// The relay has fewer log entries than this phone saw (lost data, an older backup).
  relayRolledBack,

  /// The relay doesn't know this vault any more (a wiped database).
  relayLostVault,

  /// The relay's log differs from the history this phone saw.
  relayForked,

  /// "Use Tor" is on and Tor is still connecting: nothing is sent meanwhile.
  torConnecting,

  /// "Use Tor" is on but Tor couldn't connect: nothing is sent until it does.
  torFailed,
  other,
}

/// Which server a failure concerns.
enum SyncEndpoint { relay, lightwalletd }

class SyncFailure {
  const SyncFailure({required this.kind, this.endpoint, this.detail = ''});

  final SyncFailureKind kind;

  /// The server that failed, when known (`null` for offline, database and app issues).
  final SyncEndpoint? endpoint;

  /// The technical message from Rust, shown small in the details sheet.
  final String detail;

  /// Short status for the top of Home (fits next to the vault name).
  String get statusLabel => switch (kind) {
    SyncFailureKind.offline => 'Offline',
    SyncFailureKind.lightwalletdUnreachable => 'Can\'t reach Zcash server',
    SyncFailureKind.relayUnreachable => 'Can\'t reach relay',
    SyncFailureKind.tls => 'Connection not secure',
    SyncFailureKind.timeout => 'Server not responding',
    SyncFailureKind.serverBehind => 'Server behind',
    SyncFailureKind.wrongNetwork => 'Wrong network',
    SyncFailureKind.walletDatabase => 'Storage problem',
    SyncFailureKind.updateRequired => 'Update needed',
    SyncFailureKind.relayOutdated => 'Relay outdated',
    SyncFailureKind.relayRolledBack => 'Relay lost data',
    SyncFailureKind.relayLostVault => 'Relay lost the vault',
    SyncFailureKind.relayForked => 'Relay not trusted',
    SyncFailureKind.torConnecting => 'Connecting to Tor…',
    SyncFailureKind.torFailed => 'Tor couldn\'t connect',
    SyncFailureKind.other => 'Sync failed',
  };

  /// Waiting rather than broken (shown without the error colour).
  bool get isTransient => kind == SyncFailureKind.torConnecting;

  /// Title of the details sheet.
  String get title => switch (kind) {
    SyncFailureKind.offline => 'You seem to be offline',
    SyncFailureKind.lightwalletdUnreachable => 'Can\'t reach the Zcash server',
    SyncFailureKind.relayUnreachable => 'Can\'t reach the relay',
    SyncFailureKind.tls => 'Secure connection failed',
    SyncFailureKind.timeout => 'The server isn\'t responding',
    SyncFailureKind.serverBehind => 'The Zcash server is behind',
    SyncFailureKind.wrongNetwork => 'The Zcash server is on another network',
    SyncFailureKind.walletDatabase => 'Wallet storage problem',
    SyncFailureKind.updateRequired => 'Update Zafe',
    SyncFailureKind.relayOutdated => 'The relay needs an update',
    SyncFailureKind.relayRolledBack => 'The relay lost part of the vault',
    SyncFailureKind.relayLostVault => 'The relay lost the vault',
    SyncFailureKind.relayForked => 'The relay shows another history',
    SyncFailureKind.torConnecting => 'Connecting to Tor',
    SyncFailureKind.torFailed => 'Tor couldn\'t connect',
    SyncFailureKind.other => 'Sync failed',
  };

  /// What happened and what to do, in a sentence or two.
  String get explanation => switch (kind) {
    SyncFailureKind.offline =>
      'Neither the Zcash server nor the relay answers. Check this phone\'s '
          'internet connection. Zafe keeps trying.',
    SyncFailureKind.lightwalletdUnreachable =>
      'Balances and payments can\'t update until the Zcash server (lightwalletd) '
          'answers. It may be down, or its address in Settings may be wrong. Zafe '
          'keeps trying.',
    SyncFailureKind.relayUnreachable =>
      'Proposals, votes and signatures travel through the relay, so they can\'t '
          'update until it answers. It may be down, or its address in Settings may be '
          'wrong. Zafe keeps trying.',
    SyncFailureKind.tls =>
      'The server\'s certificate isn\'t trusted, has expired or names another '
          'server, or the address uses https for a server without it. Zafe won\'t '
          'connect until this is fixed. Check the address in Settings.',
    SyncFailureKind.timeout =>
      'The server accepted the connection but didn\'t answer in time. It may be '
          'overloaded, or the connection is slow. Zafe keeps trying.',
    SyncFailureKind.serverBehind =>
      'The Zcash server is at an older block than this wallet already saw. It may '
          'still be catching up, or follow another chain. Wait, or pick another '
          'server in Settings.',
    SyncFailureKind.wrongNetwork =>
      'The Zcash server serves a different network than this vault. Pick a server '
          'for the right network in Settings.',
    SyncFailureKind.walletDatabase =>
      'This phone couldn\'t read or write the vault\'s wallet data. Your keys and '
          'funds are safe: the wallet data can be rebuilt from the chain.',
    SyncFailureKind.updateRequired =>
      'The relay or another member uses a newer version of Zafe. Update the app.',
    SyncFailureKind.relayOutdated =>
      'The relay runs an older version of Zafe than this app. Ask whoever runs it '
          'to update it.',
    SyncFailureKind.relayRolledBack =>
      'The relay has fewer entries of the vault\'s history than this phone has '
          'already seen, for example after it was restored from an older backup. '
          'Zafe ignores what it says until the history is put back. Any member whose '
          'phone has the full history can restore it.',
    SyncFailureKind.relayLostVault =>
      'The relay no longer knows this vault, for example after its database was '
          'wiped. Your funds are safe. Any member can restore the vault on the relay '
          'from the history kept on their phone. Payments that were waiting for '
          'signatures are asked for again.',
    SyncFailureKind.relayForked =>
      'The relay shows a different history of this vault than this phone saw. '
          'Zafe won\'t use it. Check with the other members whether the relay was '
          'changed or restored, and don\'t approve anything from it until you know why.',
    SyncFailureKind.torConnecting =>
      '"Use Tor" is on and Tor is still connecting. Zafe sends nothing until it '
          'is, and never connects directly instead.',
    SyncFailureKind.torFailed =>
      '"Use Tor" is on but Tor couldn\'t connect, so Zafe sends nothing: no sync, '
          'no approvals, no payments. Tor may be blocked on this network. Try again, '
          'or turn off Tor in Settings.',
    SyncFailureKind.other =>
      'Something went wrong while updating the vault. Zafe keeps trying.',
  };

  /// A member can put the vault back on the relay from this phone's copy of its history.
  bool get canRestoreRelay =>
      kind == SyncFailureKind.relayRolledBack ||
      kind == SyncFailureKind.relayLostVault;

  /// Whether changing a server address in Settings could fix it.
  bool get suggestsSettings => switch (kind) {
    SyncFailureKind.lightwalletdUnreachable ||
    SyncFailureKind.relayUnreachable ||
    SyncFailureKind.tls ||
    SyncFailureKind.serverBehind ||
    SyncFailureKind.wrongNetwork ||
    SyncFailureKind.torFailed => true,
    _ => false,
  };
}

SyncEndpoint? _endpoint(ZafeEndpoint e) => switch (e) {
  ZafeEndpoint.relay => SyncEndpoint.relay,
  ZafeEndpoint.lightwalletd => SyncEndpoint.lightwalletd,
  ZafeEndpoint.none => null,
};

/// Classifies one error. `fallback` is the server the call talked to, for errors that
/// don't name one (a sync is lightwalletd, a vault refresh the relay).
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

/// The failure Home shows, from the last wallet sync error and the last vault refresh
/// error (either may be null). Tor comes first (it stops both); both servers unreachable
/// reads as "offline"; otherwise the wallet sync's failure comes first (balances), then
/// the relay's.
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

/// "Just now", "5 min ago", "3 h ago", or a date and time for older ones; "Not yet" when
/// it never happened.
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

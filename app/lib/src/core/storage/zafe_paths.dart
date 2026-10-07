import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// App-private directories. Android backup and device transfer are disabled for the whole
/// app (AndroidManifest + data_extraction_rules), because each vault's `stateDir` holds
/// single-use FROST nonces that must never be restored (spec §9.4).
class ZafePaths {
  const ZafePaths._(this.dbDir, this._support);

  /// Wallet databases (one file per vault, named by vault id; chain data, no secrets).
  final String dbDir;
  final String _support;

  /// Everything else kept for one vault: `signing/` (nonces, pool, leader rounds), the
  /// notification snapshot and a small summary for the vault switcher.
  String vaultDir(String vaultId) => '$_support/vaults/$vaultId';

  /// Nonces, pre-published commitment nonces and the leader's signing rounds.
  Future<String> stateDir(String vaultId) async {
    final dir = Directory('${vaultDir(vaultId)}/signing');
    await dir.create(recursive: true);
    return dir.path;
  }

  /// This phone's copy of every vault's log (`<vault id>.log`): what lets the app notice
  /// a relay that lost or rewound it. Entries are stored as the relay holds them
  /// (encrypted with the vault's log key), so it needs no more protection than the relay's
  /// own storage.
  String get logDir => '$_support/log';

  /// Tor's state (guards, directory cache) when "Use Tor" is on. Covered by the app-wide
  /// backup exclusion: a restored copy would carry this phone's guard choice elsewhere.
  String get torDir => '$_support/tor';

  /// The local crash log (`core/diagnostics`). Never uploaded; the user shares it by hand.
  String get diagnosticsDir => '$_support/diagnostics';

  static ZafePaths? _cached;

  static Future<ZafePaths> get() async {
    final cached = _cached;
    if (cached != null) return cached;
    final support = await getApplicationSupportDirectory();
    return _cached = ZafePaths._(support.path, support.path);
  }

  /// Before multiple vaults, signing state and the notification snapshot lived directly
  /// under the support directory; they belong to the (single) vault that was migrated.
  Future<void> migrateLegacy(String vaultId) async {
    final target = Directory(vaultDir(vaultId));
    await target.create(recursive: true);
    final signing = Directory('$_support/signing');
    if (await signing.exists() &&
        !await Directory('${target.path}/signing').exists()) {
      await signing.rename('${target.path}/signing');
    }
    final seen = File('$_support/notifications/seen.json');
    if (await seen.exists()) await seen.rename('${target.path}/seen.json');
  }

  /// Deletes a removed vault's local files, including its wallet database.
  Future<void> deleteVault(String vaultId) async {
    final dir = Directory(vaultDir(vaultId));
    if (await dir.exists()) await dir.delete(recursive: true);
    final log = File('$logDir/$vaultId.log');
    if (await log.exists()) await log.delete();
    final db = File('$dbDir/vault-$vaultId.sqlite');
    for (final f in [db, File('${db.path}-wal'), File('${db.path}-shm')]) {
      if (await f.exists()) await f.delete();
    }
  }
}

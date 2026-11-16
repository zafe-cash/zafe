import 'dart:io';

import 'package:path_provider/path_provider.dart';

class ZafePaths {
  const ZafePaths._(this.dbDir, this._support);

  final String dbDir;
  final String _support;

  String vaultDir(String vaultId) => '$_support/vaults/$vaultId';

  Future<String> stateDir(String vaultId) async {
    final dir = Directory('${vaultDir(vaultId)}/signing');
    await dir.create(recursive: true);
    return dir.path;
  }

  String get logDir => '$_support/log';

  String get torDir => '$_support/tor';

  String get recoveryDir => '$_support/recovery';

  Future<void> deleteRecovery() async {
    final dir = Directory(recoveryDir);
    if (await dir.exists()) await dir.delete(recursive: true);
  }

  String get diagnosticsDir => '$_support/diagnostics';

  static ZafePaths? _cached;

  static Future<ZafePaths> get() async {
    final cached = _cached;
    if (cached != null) return cached;
    final support = await getApplicationSupportDirectory();
    return _cached = ZafePaths._(support.path, support.path);
  }

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

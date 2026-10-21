import 'dart:io';

import 'package:shared_preferences/shared_preferences.dart';

import 'member_names.dart';
import 'zafe_paths.dart';

/// The name a member gives themselves while setting up a vault (Create or Join). It
/// can't be published before the vault exists (a shared name is a vault log entry), so
/// it waits here until the first refresh after key generation sends it
/// (`ProposalsNotifier`). The last one used prefills the next setup.
class MyName {
  static const _lastKey = 'zafe_last_my_name';

  static Future<File> _file(String vaultId) async =>
      File('${(await ZafePaths.get()).vaultDir(vaultId)}/my_name_pending.txt');

  /// The name used in the last setup, to prefill the field.
  static Future<String> lastUsed() async {
    try {
      return (await SharedPreferences.getInstance()).getString(_lastKey) ?? '';
    } catch (_) {
      return '';
    }
  }

  /// Keeps `name` for `vaultId` until it can be published; empty keeps nothing.
  static Future<void> setPending(String vaultId, String name) async {
    final cleaned = MemberNames.clean(name);
    if (cleaned.isEmpty) return;
    try {
      await (await SharedPreferences.getInstance()).setString(
        _lastKey,
        cleaned,
      );
    } catch (_) {}
    final f = await _file(vaultId);
    await f.parent.create(recursive: true);
    await f.writeAsString(cleaned, flush: true);
  }

  static Future<String?> pending(String vaultId) async {
    try {
      final name = MemberNames.clean(
        await (await _file(vaultId)).readAsString(),
      );
      return name.isEmpty ? null : name;
    } catch (_) {
      return null;
    }
  }

  static Future<void> clearPending(String vaultId) async {
    try {
      await (await _file(vaultId)).delete();
    } catch (_) {}
  }
}

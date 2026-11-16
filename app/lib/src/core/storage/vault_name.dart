import 'dart:io';

import 'member_names.dart';
import 'zafe_paths.dart';

class VaultName {
  static Future<File> _file(String vaultId) async =>
      File('${(await ZafePaths.get()).vaultDir(vaultId)}/vault_name.txt');

  static const maxLength = 32;

  static Future<String?> read(String vaultId) async {
    try {
      final name = clean(await (await _file(vaultId)).readAsString());
      return name.isEmpty ? null : name;
    } catch (_) {
      return null;
    }
  }

  static Future<void> write(String vaultId, String name) async {
    final f = await _file(vaultId);
    final cleaned = clean(name);
    if (cleaned.isEmpty) {
      if (await f.exists()) await f.delete();
      return;
    }
    await f.parent.create(recursive: true);
    final tmp = File('${f.path}.${DateTime.now().microsecondsSinceEpoch}.tmp');
    await tmp.writeAsString(cleaned, flush: true);
    await tmp.rename(f.path);
  }

  static String clean(String name) => MemberNames.clean(name);

  static String display(String creatorName, String? local) =>
      local ?? creatorName;
}

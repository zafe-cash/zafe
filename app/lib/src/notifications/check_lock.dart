import 'dart:io';

class CheckLock {
  CheckLock(Directory dir)
    : _lock = File('${dir.path}/check.lock'),
      _again = File('${dir.path}/check.again');

  final File _lock;
  final File _again;

  static const staleAfter = Duration(minutes: 5);

  static const maxRuns = 4;

  Future<void> run(
    String? skipVaultId,
    Future<void> Function(String? skipVaultId) check,
  ) async {
    try {
      await _lock.parent.create(recursive: true);
      if (await _lock.exists() &&
          DateTime.now().difference(await _lock.lastModified()) < staleAfter) {
        await _askAgain(skipVaultId);
        return;
      }
      await _lock.writeAsString('${DateTime.now()}');
    } catch (_) {}
    try {
      String? skip = skipVaultId;
      for (var i = 0; i < maxRuns; i++) {
        await _clearAgain();
        try {
          await check(skip);
        } catch (_) {}
        final again = await _takeAgain();
        if (again == null) break;
        skip = again.isEmpty ? null : again;
        try {
          await _lock.writeAsString('${DateTime.now()}');
        } catch (_) {}
      }
    } finally {
      try {
        await _lock.delete();
      } catch (_) {}
    }
  }

  Future<void> _askAgain(String? skipVaultId) async {
    try {
      final want = skipVaultId ?? '';
      if (await _again.exists() && await _again.readAsString() != want) {
        await _again.writeAsString('');
      } else {
        await _again.writeAsString(want);
      }
    } catch (_) {}
  }

  Future<void> _clearAgain() async {
    try {
      if (await _again.exists()) await _again.delete();
    } catch (_) {}
  }

  Future<String?> _takeAgain() async {
    try {
      if (!await _again.exists()) return null;
      final skip = await _again.readAsString();
      await _again.delete();
      return skip;
    } catch (_) {
      return null;
    }
  }
}

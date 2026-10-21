import 'dart:io';

/// One vault check at a time across isolates (the periodic task, the one-off after
/// backgrounding and pushes can fire together), without losing news: a check that finds
/// another one running leaves a "check again" note, and the running one repeats once it
/// finishes. Before this a push that arrived mid-check was dropped, so a payment proposed
/// seconds after another vault's activity stayed unannounced (and without its dot in the
/// vault switcher) until the next periodic check.
class CheckLock {
  CheckLock(Directory dir)
    : _lock = File('${dir.path}/check.lock'),
      _again = File('${dir.path}/check.again');

  final File _lock;
  final File _again;

  /// A lock older than this is stale (a check that died without removing it).
  static const staleAfter = Duration(minutes: 5);

  /// Repeats at most this many times, so a stream of pushes can't keep one check alive.
  static const maxRuns = 4;

  /// Runs [check] unless another check holds the lock, in which case that one is asked
  /// to run again. [skipVaultId] is passed on; a repeat asked for by callers with
  /// different skips skips nothing. Never throws.
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
        // Keep the lock fresh for the next run.
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

  /// The note holds the vault id to skip, or nothing for "skip none".
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

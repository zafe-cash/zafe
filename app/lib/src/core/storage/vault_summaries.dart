import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import 'zafe_paths.dart';
import 'zafe_secure_store.dart';

/// What the vault switcher shows for each vault (including ones not on screen): last known
/// balance and how many payments wait for this member. A small file per vault, written by
/// the app and by background checks (files are shared across isolates, prefs caches aren't).
/// The balance itself lives in secure storage ([ZafeSecureStore.balance]), not in the
/// file; writing the file drops the plain-text balance older versions kept there.
class VaultSummaryInfo {
  const VaultSummaryInfo({
    this.balanceZat,
    this.actionable = 0,
    this.backedUp = false,
    this.syncedAt,
  });
  final BigInt? balanceZat;
  final int actionable;

  /// A backup of this device's copy exists (exported here, or restored from one).
  final bool backedUp;

  /// Last successful wallet sync (by the app or a background check).
  final DateTime? syncedAt;
}

class VaultSummaries {
  /// Bumped after each write on this isolate (the app's refreshes, and the check a push
  /// runs while the app is open), so what's derived from several vaults can re-read.
  static final ValueNotifier<int> changes = ValueNotifier(0);

  /// Whether any of [vaultIds] has a payment waiting for this member (file fields only).
  static Future<bool> anyWaiting(Iterable<String> vaultIds) async {
    for (final id in vaultIds) {
      if ((await _readFile(id)).actionable > 0) return true;
    }
    return false;
  }

  static Future<File> _file(String vaultId) async =>
      File('${(await ZafePaths.get()).vaultDir(vaultId)}/summary.json');

  static Future<VaultSummaryInfo> read(String vaultId) async {
    final file = await _readFile(vaultId);
    return VaultSummaryInfo(
      balanceZat: await _balance(vaultId),
      actionable: file.actionable,
      backedUp: file.backedUp,
      syncedAt: file.syncedAt,
    );
  }

  static Future<BigInt?> _balance(String vaultId) async {
    try {
      return await ZafeSecureStore.instance.balance(vaultId);
    } catch (_) {
      return null;
    }
  }

  /// The file's fields only (no secure storage read).
  static Future<VaultSummaryInfo> _readFile(String vaultId) async {
    try {
      final j = jsonDecode(await (await _file(vaultId)).readAsString()) as Map;
      return VaultSummaryInfo(
        actionable: (j['actionable'] as int?) ?? 0,
        backedUp: (j['backedUp'] as bool?) ?? false,
        syncedAt: j['syncedAt'] is int
            ? DateTime.fromMillisecondsSinceEpoch(j['syncedAt'] as int)
            : null,
      );
    } catch (_) {
      return const VaultSummaryInfo();
    }
  }

  // Writes are read-modify-write: serialize them (sync and refresh both write).
  static Future<void> _queue = Future.value();

  /// Updates the given fields, keeping the others. Never throws (the summary is a cache).
  static Future<void> write(
    String vaultId, {
    BigInt? balanceZat,
    int? actionable,
    bool? backedUp,
    DateTime? syncedAt,
  }) {
    final next = _queue.then((_) async {
      try {
        if (balanceZat != null && balanceZat != await _balance(vaultId)) {
          await ZafeSecureStore.instance.writeBalance(vaultId, balanceZat);
        }
        final old = await _readFile(vaultId);
        final f = await _file(vaultId);
        await f.parent.create(recursive: true);
        // A unique temp name: background checks in another isolate may write too.
        final tmp = File(
          '${f.path}.${DateTime.now().microsecondsSinceEpoch}.tmp',
        );
        await tmp.writeAsString(
          jsonEncode({
            'actionable': actionable ?? old.actionable,
            'backedUp': backedUp ?? old.backedUp,
            'syncedAt': (syncedAt ?? old.syncedAt)?.millisecondsSinceEpoch,
          }),
        );
        await tmp.rename(f.path);
        changes.value++;
      } catch (_) {}
    });
    _queue = next;
    return next;
  }
}

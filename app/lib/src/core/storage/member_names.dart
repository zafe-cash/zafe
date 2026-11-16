import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

import '../../rust/api/names.dart';
import 'zafe_paths.dart';

/// Names this member gave the vault's other signers (signing key hex → name). Local to
/// this device: never sent to the relay or other members. Encrypted backups carry them.
/// Separately, each member can share their own name through the vault log
/// (`ProposalList.sharedNames`); [merge] combines the two for display.
class MemberNames {
  /// The names to show: this device's labels win over the names members gave themselves.
  static Map<String, String> merge(
    Map<String, String> shared,
    Map<String, String> local,
  ) => SignerNames(local: local, shared: shared).merged;

  static Future<File> _file(String vaultId) async =>
      File('${(await ZafePaths.get()).vaultDir(vaultId)}/names.json');

  /// Longest name kept (it has to fit a signer row).
  static const maxLength = 32;

  /// What a name a member gives itself may hold: letters, digits, spaces and - ' ’ &,
  /// as the bridge shows them (`plain_name`); nothing that could pass for a short key.
  static final ownName = FilteringTextInputFormatter.allow(
    RegExp(r"[\p{Alphabetic}\p{N} '’&-]", unicode: true),
  );

  static Future<Map<String, String>> read(String vaultId) async {
    try {
      final j = jsonDecode(await (await _file(vaultId)).readAsString()) as Map;
      return {
        for (final e in j.entries)
          if (e.key is String && e.value is String)
            e.key as String: e.value as String,
      };
    } catch (_) {
      return const {};
    }
  }

  static Future<void> write(String vaultId, Map<String, String> names) async {
    final f = await _file(vaultId);
    await f.parent.create(recursive: true);
    final tmp = File('${f.path}.${DateTime.now().microsecondsSinceEpoch}.tmp');
    await tmp.writeAsString(jsonEncode(names), flush: true);
    await tmp.rename(f.path);
  }

  /// The names as the bridge's list (backups, CSV export).
  static List<SignerName> toSigners(Map<String, String> names) => [
    for (final e in names.entries) SignerName(keyHex: e.key, name: e.value),
  ];

  /// Names from the bridge's list (a restored backup), cleaned like typed names; entries
  /// that clean to nothing or whose key isn't hex are dropped.
  static Map<String, String> fromSigners(Iterable<SignerName> signers) {
    final hex = RegExp(r'^[0-9a-f]{64}$');
    return {
      for (final s in signers)
        if (hex.hasMatch(s.keyHex) && clean(s.name).isNotEmpty)
          s.keyHex: clean(s.name),
    };
  }

  /// `name` trimmed and capped; empty removes the name.
  static String clean(String name) {
    final t = name.trim().replaceAll(RegExp(r'\s+'), ' ');
    return t.length > maxLength ? t.substring(0, maxLength) : t;
  }
}

/// Who each signer is to this phone. A **label** is a name this phone's owner gave a key
/// here; a **claim** is the name a member gave itself in the vault log, which any member
/// can pick, so it never shows without the member's short key ([memberLabel]). A claim
/// that reads like a label of another key is left out: that member shows as its short key.
class SignerNames {
  SignerNames({
    Map<String, String> local = const {},
    Map<String, String> shared = const {},
  }) : local = {
         for (final e in local.entries)
           if (e.value.trim().isNotEmpty) e.key: e.value,
       },
       shared = _claims(local, shared);

  static final none = SignerNames();

  final Map<String, String> local;

  /// Claims still shown: none for a key this phone labelled, none that reads like
  /// another key's label.
  final Map<String, String> shared;

  static Map<String, String> _claims(
    Map<String, String> local,
    Map<String, String> shared,
  ) {
    String fold(String s) => s.trim().toLowerCase();
    final labels = {
      for (final e in local.entries)
        if (e.value.trim().isNotEmpty) fold(e.value),
    };
    return {
      for (final e in shared.entries)
        if (e.value.trim().isNotEmpty &&
            (local[e.key]?.trim().isEmpty ?? true) &&
            !labels.contains(fold(e.value)))
          e.key: e.value,
    };
  }

  /// The name alone, label first (signer rows, which show the key under it).
  String? nameOf(String keyHex) => local[keyHex] ?? shared[keyHex];

  /// Whether [nameOf] is the member's own claim rather than this phone's label.
  bool isClaim(String keyHex) =>
      !local.containsKey(keyHex) && shared.containsKey(keyHex);

  /// Key → name, labels first (backups of names, CSV export, notifications, which add
  /// the key to every name).
  Map<String, String> get merged => {...shared, ...local};
}

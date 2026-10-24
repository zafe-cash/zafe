import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// One vault's secrets on this device. Each vault has its own member identity, so the relay
/// can't link one person's memberships across vaults.
class StoredVault {
  const StoredVault({
    required this.id,
    this.identity,
    this.invite,
    this.material,
  });

  /// The vault id (hex), from the invite onward.
  final String id;
  final Uint8List? identity;
  final String? invite;

  /// FROST share, vault secret, log key and descriptor; set after key generation.
  final Uint8List? material;

  bool get ready => material != null;
}

/// Platform secure storage (iOS Keychain / Android Keystore) for Zafe's secrets, per vault:
/// identity seeds, the invite (not secret, kept so setup can resume), the vault material and
/// the wallet database key.
/// `zafe_vaults` lists the vault ids in the order they were added.
class ZafeSecureStore {
  ZafeSecureStore._();
  static final instance = ZafeSecureStore._();

  static const _registryKey = 'zafe_vaults';

  // The single-vault layout before multiple vaults (migrated on first read).
  static const _legacyIdentityKey = 'zafe_identity_seeds';
  static const _legacyInviteKey = 'zafe_invite';
  static const _legacyMaterialKey = 'zafe_vault_material';

  final _storage = _PatientStorage(
    const FlutterSecureStorage(
      iOptions: IOSOptions(
        accessibility: KeychainAccessibility.first_unlock_this_device,
      ),
    ),
  );

  String _key(String id, String field) => 'zafe_vault_${id}_$field';

  Future<Uint8List?> _readBytes(String key) async {
    final value = await _storage.read(key: key);
    return value == null ? null : base64Decode(value);
  }

  Future<void> _writeBytes(String key, List<int> bytes) =>
      _storage.write(key: key, value: base64Encode(bytes));

  Future<List<String>> vaultIds() async {
    final raw = await _storage.read(key: _registryKey);
    return raw == null ? [] : List<String>.from(jsonDecode(raw) as List);
  }

  Future<void> _writeIds(List<String> ids) =>
      _storage.write(key: _registryKey, value: jsonEncode(ids));

  Future<StoredVault> read(String id) async => StoredVault(
    id: id,
    identity: await _readBytes(_key(id, 'identity')),
    invite: await _storage.read(key: _key(id, 'invite')),
    material: await _readBytes(_key(id, 'material')),
  );

  /// Every vault on this device, in the order added.
  Future<List<StoredVault>> readAll() async => [
    for (final id in await vaultIds()) await read(id),
  ];

  /// Adds a vault (identity and invite; material comes after key generation). The
  /// creator of a vault on a private relay keeps its `createToken` with the invite until
  /// the material (which then holds it) is saved: every setup step needs it.
  Future<void> add({
    required String id,
    required List<int> identity,
    required String invite,
    String? createToken,
  }) async {
    await _writeBytes(_key(id, 'identity'), identity);
    await _storage.write(key: _key(id, 'invite'), value: invite);
    if (createToken != null) {
      await _storage.write(key: _key(id, 'createToken'), value: createToken);
    }
    final ids = await vaultIds();
    if (!ids.contains(id)) await _writeIds([...ids, id]);
  }

  Future<void> writeMaterial(String id, List<int> material) =>
      _writeBytes(_key(id, 'material'), material);

  /// The create token the creator made vault `id` with (see [add]); null on an open relay
  /// and for joiners.
  Future<String?> setupCreateToken(String id) =>
      _storage.read(key: _key(id, 'createToken'));

  /// The vault's material holds the token now.
  Future<void> forgetSetupCreateToken(String id) =>
      _storage.delete(key: _key(id, 'createToken'));

  static const _recoveryKey = 'zafe_recovery_identity';

  /// The identity a new phone made to take over a lost signer's seat, kept until the
  /// seat has moved and the vault is saved (spec §10.1).
  Future<Uint8List?> readRecoveryIdentity() => _readBytes(_recoveryKey);

  Future<void> writeRecoveryIdentity(List<int> seeds) =>
      _writeBytes(_recoveryKey, seeds);

  Future<void> deleteRecoveryIdentity() async {
    await _storage.delete(key: _recoveryKey);
    await _storage.delete(key: _recoveryLinkKey);
  }

  static const _recoveryLinkKey = 'zafe_recovery_seat_link';

  /// The vault link (`zafe-seat-v1:...`) a co-signer gave the recovering phone: which
  /// relay and mailbox to recover on. Kept with the recovery identity.
  Future<String?> readRecoveryLink() => _storage.read(key: _recoveryLinkKey);

  Future<void> writeRecoveryLink(String? link) => link == null
      ? _storage.delete(key: _recoveryLinkKey)
      : _storage.write(key: _recoveryLinkKey, value: link);

  /// Last known balance in zatoshis, for the vault switcher (kept here rather than in
  /// the summary file so it isn't readable outside the keystore-backed storage).
  Future<BigInt?> balance(String id) async {
    final raw = await _storage.read(key: _key(id, 'balance'));
    return raw == null ? null : BigInt.tryParse(raw);
  }

  Future<void> writeBalance(String id, BigInt zat) =>
      _storage.write(key: _key(id, 'balance'), value: zat.toString());

  /// Removes a vault's secrets from this device (it stays a member on the relay).
  Future<void> remove(String id) async {
    for (final field in [
      'identity',
      'invite',
      'material',
      'walletKey',
      'balance',
      'createToken',
    ]) {
      await _storage.delete(key: _key(id, field));
    }
    await _writeIds([
      for (final x in await vaultIds())
        if (x != id) x,
    ]);
  }

  /// Moves the pre-multi-vault single slot into the registry under `id` (the invite's vault
  /// id, which the caller parses). Returns the migrated vault, or null if there was none.
  Future<StoredVault?> migrateLegacy(
    String Function(String invite) vaultIdOf,
  ) async {
    final invite = await _storage.read(key: _legacyInviteKey);
    final identity = await _readBytes(_legacyIdentityKey);
    if (invite == null || identity == null) return null;
    final id = vaultIdOf(invite);
    await add(id: id, identity: identity, invite: invite);
    final material = await _readBytes(_legacyMaterialKey);
    if (material != null) await writeMaterial(id, material);
    for (final key in [
      _legacyIdentityKey,
      _legacyInviteKey,
      _legacyMaterialKey,
    ]) {
      await _storage.delete(key: key);
    }
    return read(id);
  }
}

/// `FlutterSecureStorage` with a timeout and one retry per call. On Android a call can
/// occasionally get no reply at all: seen right after key generation, while a background
/// check's engine used the same storage (its plugin instance logged "decryption failed"
/// at the same moment). The first sync then waited forever on `walletKey`; a second call
/// answered at once. So a lost reply costs [_timeout] instead of hanging the caller.
class _PatientStorage {
  const _PatientStorage(this._inner);
  final FlutterSecureStorage _inner;

  static const _timeout = Duration(seconds: 10);

  Future<T> _call<T>(String what, Future<T> Function() op) async {
    try {
      return await op().timeout(_timeout);
    } on TimeoutException {
      debugPrint(
        'secure storage: no answer to $what in ${_timeout.inSeconds}s, retrying',
      );
      return await op().timeout(_timeout);
    }
  }

  Future<String?> read({required String key}) =>
      _call('read', () => _inner.read(key: key));

  Future<void> write({required String key, required String? value}) =>
      _call('write', () => _inner.write(key: key, value: value));

  Future<void> delete({required String key}) =>
      _call('delete', () => _inner.delete(key: key));
}

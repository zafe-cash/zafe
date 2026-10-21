import 'dart:typed_data';

import '../../rust/api/vault.dart' as rust;

/// The key that encrypts a vault's wallet database (SQLCipher), derived in Rust from this
/// device's identity seeds and the vault id. Nothing is stored or read from secure storage:
/// the old random per-vault key needed a storage call on the sync path, and on Android that
/// call could go unanswered for a minute right after key generation (a background engine
/// using the same storage), stalling the first sync. The wallet only caches chain data, so
/// a database made with an older stored key is deleted on open and syncs again.
Uint8List walletKeyFor(List<int> identitySeeds, String vaultId) =>
    rust.walletDbKey(seeds: identitySeeds, vaultId: vaultId);

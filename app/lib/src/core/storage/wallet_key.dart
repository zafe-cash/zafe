import 'dart:typed_data';

import '../../rust/api/vault.dart' as rust;

Uint8List walletKeyFor(List<int> identitySeeds, String vaultId) =>
    rust.walletDbKey(seeds: identitySeeds, vaultId: vaultId);

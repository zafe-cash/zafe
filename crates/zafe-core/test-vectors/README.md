# Test vectors

- `zip2005_use_qsk.json`: Zafe's ZIP 2005 `use_qsk = true` derivation vectors, independently
  checked by `scripts/check_zip2005_vectors.py`. Not official. Regenerate with
  `ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors`.
- `zcash-sign_ironwood_v6.pczt` (used by `tests/zcash_sign_crosscheck.rs`): copied from
  ZcashFoundation/frost-tools (`zcash-sign/tests/fixtures/ironwood_v6.pczt`, commit 06c0dbd,
  MIT/Apache-2.0). A real, proven, unsigned v6 Ironwood PCZT that became testnet transaction
  `a5533fe75575b09d8986a05005d2c0528cf45a1a7e4cc71b304ae76a9e14487d`, signed with a 2-of-3
  re-randomized FROST signature against sighash
  `7d48149e6ae74a301bc74c7e1a5af48c52c0d20550a086899db07555e73f53a5`.

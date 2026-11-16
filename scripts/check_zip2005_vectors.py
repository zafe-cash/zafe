#!/usr/bin/env python3
import hashlib
import json
import sys
from pathlib import Path

P = 0x40000000000000000000000000000000224698FC094CF91B992D30ED00000001
Q = 0x40000000000000000000000000000000224698FC0994A8DD8C46EB2100000001

QK_CONTEXT = "Zcash ZIP 2005 qk-derivation v1"


def prf_expand(key: bytes, t: bytes) -> bytes:
    return hashlib.blake2b(key + t, digest_size=64, person=b"Zcash_ExpandSeed").digest()


def to_base(b: bytes) -> int:
    return int.from_bytes(b, "little") % P


def to_scalar(b: bytes) -> int:
    return int.from_bytes(b, "little") % Q


def i2leosp256(x: int) -> bytes:
    return x.to_bytes(32, "little")


_IV = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
    0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
]
_PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
CHUNK_START, CHUNK_END, ROOT = 1 << 0, 1 << 1, 1 << 3
DERIVE_KEY_CONTEXT, DERIVE_KEY_MATERIAL = 1 << 5, 1 << 6
M32 = 0xFFFFFFFF


def _rotr(x, n):
    return ((x >> n) | (x << (32 - n))) & M32


def _g(s, a, b, c, d, mx, my):
    s[a] = (s[a] + s[b] + mx) & M32
    s[d] = _rotr(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & M32
    s[b] = _rotr(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b] + my) & M32
    s[d] = _rotr(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & M32
    s[b] = _rotr(s[b] ^ s[c], 7)


def _compress(cv, block, block_len, counter, flags):
    m = [int.from_bytes(block[4 * i:4 * i + 4], "little") for i in range(16)]
    s = cv[:] + _IV[:4] + [counter & M32, (counter >> 32) & M32, block_len, flags]
    for r in range(7):
        _g(s, 0, 4, 8, 12, m[0], m[1])
        _g(s, 1, 5, 9, 13, m[2], m[3])
        _g(s, 2, 6, 10, 14, m[4], m[5])
        _g(s, 3, 7, 11, 15, m[6], m[7])
        _g(s, 0, 5, 10, 15, m[8], m[9])
        _g(s, 1, 6, 11, 12, m[10], m[11])
        _g(s, 2, 7, 8, 13, m[12], m[13])
        _g(s, 3, 4, 9, 14, m[14], m[15])
        if r < 6:
            m = [m[i] for i in _PERM]
    return [s[i] ^ s[i + 8] for i in range(8)]


def _hash_single_chunk(key_words, data: bytes, mode_flags: int) -> bytes:
    assert len(data) <= 1024, "single-chunk BLAKE3 only"
    blocks = [data[i:i + 64] for i in range(0, len(data), 64)] or [b""]
    cv = key_words
    for i, block in enumerate(blocks):
        flags = mode_flags
        if i == 0:
            flags |= CHUNK_START
        if i == len(blocks) - 1:
            flags |= CHUNK_END | ROOT
        cv = _compress(cv, block.ljust(64, b"\0"), len(block), 0, flags)
    return b"".join(w.to_bytes(4, "little") for w in cv)


def blake3_hash(data: bytes) -> bytes:
    return _hash_single_chunk(_IV, data, 0)


def blake3_derive_key(context: str, material: bytes) -> bytes:
    ctx_key = _hash_single_chunk(_IV, context.encode(), DERIVE_KEY_CONTEXT)
    words = [int.from_bytes(ctx_key[4 * i:4 * i + 4], "little") for i in range(8)]
    return _hash_single_chunk(words, material, DERIVE_KEY_MATERIAL)


def main() -> int:
    assert blake3_hash(b"").hex() == (
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    ), "BLAKE3 self-test failed"

    root = Path(__file__).resolve().parent.parent
    data = json.loads((root / "crates/zafe-core/test-vectors/zip2005_use_qsk.json").read_text())
    assert data["qk_context"] == QK_CONTEXT

    for i, v in enumerate(data["vectors"]):
        sk = bytes.fromhex(v["sk"])
        ak = bytes.fromhex(v["ak"])
        assert ak[31] & 0x80 == 0, f"vector {i}: ak not even-y"

        nk = to_base(prf_expand(sk, bytes([0x07])))
        qsk = prf_expand(sk, bytes([0x0C]))[:32]
        qk = blake3_derive_key(QK_CONTEXT, qsk)
        rivk_ext = to_scalar(prf_expand(qk, bytes([0x0D]) + ak + i2leosp256(nk)))

        checks = {
            "nk": i2leosp256(nk).hex(),
            "qsk": qsk.hex(),
            "qk": qk.hex(),
            "rivk_ext": i2leosp256(rivk_ext).hex(),
            "fvk": (ak + i2leosp256(nk) + i2leosp256(rivk_ext)).hex(),
        }
        for name, expected in checks.items():
            if v[name] != expected:
                print(f"MISMATCH vector {i} {name}:\n  rust   {v[name]}\n  python {expected}")
                return 1

    print(f"OK: {len(data['vectors'])} vectors independently verified "
          "(nk, qsk, qk, rivk_ext, fvk)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

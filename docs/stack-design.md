# Zafe stack: design notes behind `stack-plan.md`

Research from 2026-10-01. Each repo's code was read at HEAD with `gh`; anything not
checked in code is marked **unverified**. The checklist is `docs/stack-plan.md`. This
file holds the reasoning and the reference material that checklist points to.

## 1. What "the stack" is

Today Zafe is one app over `zafe-core`. The goal is for the parts every shielded multisig
needs to become libraries and specs that other wallets use. The Zafe app then becomes one
skin over them.

What a multisig needs, and who should own each part:

| Layer | Owner (target) | Why |
|---|---|---|
| FROST ciphersuite, DKG (COCKTAIL), refresh/repair | **ZF** (`frost`, plus `frost-redpallas` after frost#963) | ZF builds these in-house. We review, test and supply vectors |
| ZIP 2005 vault keys from `ak` + `sk` (`use_qsk = true`) | **orchard** (orchard#475), checked by our vectors | One constructor every wallet uses; our vectors cross-check it |
| PCZT co-signer check (recipient/amount/memo via OVK, change, exact fee) | **librustzcash `pczt` Verifier role** (ZIP 374) and frost-tools #580 | Every signer needs it, and today only Zafe has it |
| Coordination: vault log, proposals, votes, note reservation, one-tap commitment pools, leader resumability, invites, backup | **Zafe** (`zafe-protocol` + `zafe-coordinator`), specified as a draft ZIP | Nobody else has this layer. It is the "brain" |
| Transport | Pluggable: the Zafe relay (store-and-forward, long poll, push) or frostd (interactive rounds only) | Wallets choose. frostd can't hold a log (§4) |
| Wallet DB, sync, PCZT building, proving, broadcast | **The host wallet** | Zodl and Vizor already own these, and we must not duplicate them |
| UI | Each wallet (the Zafe app is the reference skin) | |

So "canonical" realistically means two things:
- Upstream what belongs upstream: vectors, the verifier, the ZIP 2005 constructor, frostd features.
- Own the coordination protocol as an open, documented spec with test vectors and
  drop-in crates.

## 2. Target crates

```
zafe-proto (wire; no Zcash deps)  <── zafe-relay (server; features: fcm, limits)
     ^
zafe-protocol (pure, sync; features: prover, multicore)
     ^
zafe-coordinator (async flows over traits; features: tokio [default], fs-store)
     ^                     ^                        ^
zafe-relay-client    zafe-wallet-sqlite        zafe-backup (feature: suggest = zxcvbn + bip0039)
(feature: tor)       (features: sqlcipher, tor, mempool-watch)
     \______ zafe-net (tor.rs + net.rs; one process-wide Tor policy) ______/
                             ^
zafe-core (facade: re-exports + batteries for the Zafe app)
     ^
rust_lib_zafe (FRB), zafe-cli; later zafe-uniffi (Kotlin/Swift), zafe-ffi (C ABI)
```

### zafe-protocol

**Moved as is:** today's `keys`, `keygen`, `signing`, `tx`, `verify`, `session` (with
`NonceStore`/`PoolStore`) and `vault`.

**New modules:**
- `network.rs`: `ZafeNetwork`, moved from `wallet.rs`.
- `material.rs`: `VaultMaterial`, with its own error type.
- `codec.rs`: the request hash, commitments hash, the codecs for requests, own shares
  and used commitments, and the wire structs that are private in node today.
- `policy.rs`: `expectations`, `orchard_receiver`, `note_holds`, `forget_closed*`,
  `is_ready`, `review`.

**Sans-IO steps pulled out of node:** `build_vote`, `aggregate_ready`, `answer_request`,
`classify_inbox`, `filter_shares`, `proposal_event`, and a `KeygenCeremony` state
machine.

**Feature `prover`** gates the proving key, prove and extract, so a signer-only build
exists for hardware wallets.

No tokio, reqwest, rusqlite or tonic. CI checks this with
`cargo check -p zafe-protocol --no-default-features`.

### zafe-coordinator

- node.rs flows, made generic over the traits below.
- The leader's resumable send. It lives in the bridge today (`send_proposal` /
  `send_with_progress` in `app/rust/src/api/proposals.rs`) and is duplicated in the CLI.
- `maintain()`: top up the pool, forget closed proposals, reserve notes. Today this
  lives in the bridge.

Without these in the library, a second wallet would have to re-implement
safety-critical sequencing itself.

### Dependency pins

Library crates use **semver ranges** (`orchard = "0.16"`, `pczt = "0.10"`,
`zcash_client_backend = "0.25"`; pre-releases need an explicit `-pre` requirement). Exact pins stay in the app's lockfile.

### Trait boundaries (sketch; derived from how node.rs uses each dependency)

```rust
pub trait Mailbox {          // envelopes: DKG + interactive signing
    fn send(&self, env: &Envelope) -> impl Future<Output = Result<(), TransportError>> + Send;
    fn inbox(&self, mailbox: MailboxId, after: u64) -> impl Future<Output = Result<Vec<(u64, Envelope)>, TransportError>> + Send;
    fn ack(&self, mailbox: MailboxId, cursors: &[u64]) -> impl Future<Output = Result<u64, TransportError>> + Send; // Ok(0) = unsupported
    fn wait(&self, mailbox: MailboxId, log_len: u64, inbox_after: u64) -> impl Future<Output = Result<Option<WaitResponse>, TransportError>> + Send;
}
pub trait VaultLog {         // durable, append-only, compare-and-swap on head
    fn append(&self, entry: &LogEntry) -> impl Future<Output = Result<AppendResult, TransportError>> + Send;
    fn read(&self, mailbox: MailboxId, from: u64) -> impl Future<Output = Result<Vec<LogEntry>, TransportError>> + Send;
}
pub trait Membership { /* create / join / seal / members: setup on the Zafe relay */ }

/// Contract: IO-finalized PCZT, OvkPolicy::Sender, Ironwood change,
/// expiry = vault::expiry_height(target, window), locked notes never selected.
pub trait VaultWalletOps {   // Zodl: backed by createPcztFromProposal; Zafe: zcash_client_sqlite
    type Params: Parameters + Clone;
    fn params(&self) -> &Self::Params;
    fn chain_height(&self) -> Result<Option<u32>, WalletError>;
    fn tx_mined(&self, txid: &[u8; 32]) -> Result<bool, WalletError>;
    fn reserve(&mut self, holds: &[NoteHold]) -> Result<usize, WalletError>;
    fn propose(&mut self, payments: &[PaymentRequest], expiry_blocks: u32) -> Result<Pczt, WalletError>;
    fn propose_sweep(&mut self, nullifiers: &[[u8; 32]], to: ZcashAddress, expiry_blocks: u32) -> Result<Pczt, WalletError>;
}
pub trait ChainClient {      // MUST read the whole mempool, never look up a txid (privacy)
    fn tip(&mut self) -> impl Future<Output = Result<u32, ChainError>> + Send;
    fn mempool_txids(&mut self) -> impl Future<Output = Result<BTreeSet<[u8; 32]>, ChainError>> + Send;
    fn send_raw(&mut self, raw: &[u8]) -> impl Future<Output = Result<Result<(), String>, ChainError>> + Send;
}
pub trait LeaderStore { /* <id>.req, <id>.own, used_commitments (unreadable => Err, never empty) */ }
pub trait SentTxStore { /* raw bytes of sent txs, for resend */ }
pub trait Runtime { fn now_unix(&self) -> u64; fn sleep(..); fn prove(..); } // tokio impl by default
```

Hardening that comes with the split:
- `propose` runs `verify_pczt` on its own PCZT before logging it. Otherwise a foreign
  wallet adapter could publish proposals that every member rejects.
- Used commitments are persisted *before* a request is sent.
- The string match for insufficient funds in `wallet.rs` (`e.contains("InsufficientFunds")`)
  becomes a typed variant.

### Coupling to remove (file:line at 2026-10-01)

- **node.rs talks to concrete adapters:**
  - `&RelayClient` in about 25 async fns.
  - `VaultWallet` or the lightwalletd `Client` in `propose` 969, `invalidate` 1045,
    `reserve_notes` 1111, `send_raw` 1628, `broadcast` 1611, `send_ready` 1521 and
    `finalize` 2152.
  - tokio `sleep`/`spawn_blocking`, `SystemTime` and `std::fs` (`SentTxs` 1647-1685)
    are called inside node.
- **Errors:** `NodeError::{Relay, Wallet}` wrap concrete adapter errors, and the
  bridge's `error.rs` depends on them staying typed (`UpdateRequired`, `RelayOutdated`,
  `RelayStorageFull`, `FundsReserved`).
- **Reverse edges:** `backup.rs:41` → node, `history.rs:9` → wallet, `mempool.rs:29` →
  wallet, `net` ↔ `tor`.
- **One `Cargo.toml` for everything:** any user of `keys` or `verify` drags in tonic,
  arti, SQLCipher with vendored OpenSSL, reqwest and zxcvbn.
- **Test gap:** the flow propose → approve → request → respond → finalize, and one-tap
  `send_ready`, run only in Docker-gated tests (`regtest_e2e`, `bridge_e2e`). Build an
  in-memory harness before the risky refactor steps:
  - MemoryRelay, with compare-and-swap and the no-self-message 403;
  - MockWallet over `tests/common::build_pczt`;
  - MockChain.

## 3. Upstream contribution map

Ordered by value ÷ effort. "Ask first" means a comment or issue before any code
(frost-tools' CONTRIBUTING requires it for large PRs).

| # | Where | What | Status (2026-10-01) | Our PR |
|---|---|---|---|---|
| 1 | frost-tools (conradoplg's suggestion on [frost#1094](https://github.com/ZcashFoundation/frost/issues/1094)) | ZIP 2005 `use_qsk` vectors | Invited. No ZIP 2005 vectors exist anywhere: [zcash-test-vectors](https://github.com/zcash/zcash-test-vectors) has none, and #129 covers only QR note commitments | `zip2005_use_qsk.json` plus the Python checker. Ask where they go (likely `zcash-sign/tests/fixtures`, as [#593](https://github.com/ZcashFoundation/frost-tools/pull/593) did). Later, a generator in zcash-test-vectors |
| 2 | ZcashFoundation/frost book | `book/src/zcash/technical-details.md` says to throw `sk` away, which contradicts ZIP 2005 | Already on our tracker | Small docs PR |
| 3 | frost-tools [#579](https://github.com/ZcashFoundation/frost-tools/pull/579)/[#580](https://github.com/ZcashFoundation/frost-tools/pull/580) (confirm PCZT contents, [#333](https://github.com/ZcashFoundation/frost-tools/issues/333)) | A participant checks the sighash and is shown the PCZT contents | Open since 2026-01 by conradoplg, unreviewed | Review first. Then follow-ups adding our checks: OVK recovery, change trial-decryption, exact ZIP 317 fee, spends checked before outputs |
| 4 | frost [#1094](https://github.com/ZcashFoundation/frost/issues/1094) | External-randomizer API (un-deprecate `sign()`, or add `sign_with_randomizer`) | conradoplg is leaning towards un-deprecating | Offer the PR, plus docs saying α must stay secret |
| 5 | zips [#895](https://github.com/zcash/zips/pull/895) (ZIP 312 keygen) | `sk` agreement, randomizer | Open since 2024-08. str4d (2026-08-28): a plain hash of the `sk_i` lets the last participant bias `sk`; the fix is to hash the COCKTAIL transcript. **Ours has the same issue (§7)** | Review comments: a builder-chosen α as an allowed option (accepted on #1094); our commit-then-reveal fix once it's in |
| 6 | frost-tools [#433](https://github.com/ZcashFoundation/frost-tools/issues/433), [#266](https://github.com/ZcashFoundation/frost-tools/issues/266), [#311](https://github.com/ZcashFoundation/frost-tools/issues/311) | frostd: group and threshold in the DKG; push or long poll; share repair | Open and unassigned. frostd is polling-only, in-memory, with 24 h sessions | Ask first. #433 is the easy start. Then a `/wait`-style long poll (our relay's design) as a step before websockets |
| 7 | frost-tools [#488](https://github.com/ZcashFoundation/frost-tools/issues/488), [#498](https://github.com/ZcashFoundation/frost-tools/issues/498), [#499](https://github.com/ZcashFoundation/frost-tools/issues/499) | Small client bugs | Open | Reputation PRs |
| 8 | zcash-test-vectors [#130](https://github.com/zcash/zcash-test-vectors/pull/130) | Ironwood v6 sighash vectors | Open | Cross-check with our sighash code and report as a review |
| 9 | orchard [#475](https://github.com/zcash/orchard/pull/475) + `zcash-sign generate` | Quantum-recoverable FROST FVK constructor | Draft by conradoplg, parked by ECC until Daira's issues are resolved. frost-tools still uses the non-recoverable one ([#591](https://github.com/ZcashFoundation/frost-tools/issues/591)) | Ask conradoplg. Offer `from_ak_and_sk_zip2005(ak, sk)` plus our vectors, then switch `zcash-sign generate` to it |
| 10 | librustzcash `pczt` | A semantic Verifier. ZIP 374 defines the role, but the crate's `roles/verifier` only exposes the parsed bundles | ECC-owned (str4d, nuttycom); review is slow | Issue first: `check_payments(ufvk, expected)`, change ownership and fee checks, built from our `verify.rs`. The ZIP 374 **Redactor** is also where α gets stripped before a PCZT leaves the vault |
| 11 | frost [#963](https://github.com/ZcashFoundation/frost/issues/963) | Move redpallas into the frost repo | Blocked on exposing reddsa internals; reddsa#245 merged | Offer help; adopt the moment it ships |
| 12 | frost [#1032](https://github.com/ZcashFoundation/frost/pull/1032)/[#1033](https://github.com/ZcashFoundation/frost/issues/1033), C2SP [#299](https://github.com/C2SP/C2SP/issues/299) | COCKTAIL-DKG | WIP draft. Spec v0.2.1 already defines `COCKTAIL(Pallas, BLAKE2b-512)` with CCTV vectors | Review; cross-check the vectors; prototype the zips#895 `sk` payload layer on Pallas. Don't build a rival DKG |
| 13 | zips (new draft) | Shielded multisig coordination: vault log, verification rules, commitment pools, backup format | Nothing exists. aryaethn said he'd write a FROST backup ZIP (**unverified**: no draft found) | Forum post first. Then a draft named `draft-<owner>-shielded-multisig-coordination`, category 300-399. Invite co-authors |
| — | Don't take | frost [#1067](https://github.com/ZcashFoundation/frost/issues/1067) (claimed by aryaethn) | | |

**Contribution norms in frost and frost-tools:**
- PR titles follow Conventional Commits; PRs are squash-merged.
- No CLA or DCO; MIT/Apache.
- New dependencies need cargo-vet (frost-tools' `supply-chain/`).

**Precedent for an outside contributor (aryaethn):**
1. A forum design thread.
2. A scoping issue with on-chain proof.
3. Two small, split PRs.
4. Merged in 4 days, with conradoplg editing the branch himself.

**ZIPs:**
- Process: forum discussion, then a PR named `draft-<owner>-<name>`. The editors assign the number.
- Editors: str4d, Daira-Emma Hopwood, nuttycom; Arya and Marek (ZF); Mark Henderson and
  Sam H. Smith (Shielded Labs); Sean Bowe; Tal Derei; Dev Ojha.

## 4. frostd interop

**What frostd is** ([frost-tools/frostd](https://github.com/ZcashFoundation/frost-tools)):
- JSON over HTTPS; every route is a POST.
- `/challenge`, then `/login`: XEdDSA over an X25519 comm key gives a bearer token valid for 1 h.
- `/create_new_session {pubkeys, message_count}`, `/list_sessions`, `/get_session_info`.
- `/send {session_id, recipients, msg ≤ 65535 B}`.
- `/receive {session_id, as_coordinator}`, which drains the queue: no cursor, no ack.
- `/close_session`.
- Everything is in memory; sessions last 24 h; clients encrypt with Noise K.

**What fits:** a `FrostdMailbox` adapter can carry **interactive signing rounds** only.
- The leader opens one session per request.
- Envelopes pass through as Zafe's tagged, signed, HPKE-sealed bytes.
- Received messages are persisted locally *before* returning, because drain-on-read plus
  a crash means lost shares.
- Cursors are synthesized, and `ack` returns 0.
- Login uses a comm key derived from the identity seed under its own KDF label.

**What doesn't:**
- frostd **can't** be the vault log (no durable store, no compare-and-swap) or the
  membership service (no sealing or join tokens). The log stays on a `VaultLog`.
- Talking to *frost-client* participants is a bigger job:
  - Noise sessions and frost-client's JSON messages.
  - frost-core 2.2 vs our 3.0 serialization.
  - frost-client signs blind (it never sees the PCZT), and a Zafe member must refuse to do that.
  - Only workable if the PCZT rides in `aux_msg`, which limits it to about 32 KB raw.
- DKG interop with frost-client is not feasible: no echo round, no `sk` agreement, and the
  legacy derivation.

**Better long-term:** get the relay features we need into frostd itself (#266 long poll,
persistence), so one server serves both.

## 5. What a spec must pin down (for a second implementation)

**Global rules:**
- postcard 1.x encoding (an enum's variant index is its wire value);
- the u16 little-endian version tag;
- decoders accept only known versions and fail with `UnsupportedVersion` otherwise; which
  old versions each format still reads is listed in AGENTS.md ("Vault log and versions").

| Format | Still undocumented |
|---|---|
| ENVELOPE | `Header` field order. `Recipient`/`Kind` indices (Join=0 … SignatureShares=10; Approval and LogEntry are test-only, so mark them reserved). Signed bytes (domain `Zafe envelope signature v1`). HPKE suite X25519-HKDF-SHA256 / ChaCha20Poly1305, base mode, info `Zafe HPKE v1`, AAD = header. `ReplayGuard` seq. Raw payload per Kind |
| LOG_ENTRY | Header encoding. XChaCha20-Poly1305 with AAD = header. Signature domain. Hash personalization `Zafe_LogEntryHsh`. Chain rules and the relay's compare-and-swap. Log-key rotation is described in the spec but **not implemented** |
| VAULT_EVENT | Variant indices. **The replay rules work like consensus** (`VaultState::apply`, `apply_versioned`): vote counting, rejection threshold n−t+1, author-only cancel, `NotesInUse`, `assign_commitments` order and caps, `ready_group`/`completed_by`, final votes, name rules, what gets ignored. Needs log → state test vectors |
| DESCRIPTOR | Field order, hash personalization, signing message, member ordering, `member_identifier`, DKG transcript hash |
| RELAY_API | Endpoints (incl. the labelled routes and `GET /v1/info`), `Signed<T>`, 300 s clock skew, the `zafe-create-token` header, status codes (403 vs 404 on `/v1/wait`, a bare 404 vs "unknown mailbox", 401, 426, 429, 507), limits |
| INVITE, IDENTITY_SEEDS | Seed → Ed25519/X25519 derivation, fingerprint, and **the safety words algorithm** (users compare them across implementations). Consider adding an invite checksum |
| DKG_ROUND1, SIGNING_REQUEST, SIGNATURE_SHARES | Message structs, ceremony order, request hash over the tagged bytes, deterministic signer choice, α = the PCZT's α |
| BACKUP, VAULT_MATERIAL | Needed for cross-wallet restore (the backup objection ZCG raised) |
| MOVE_MESSAGE, MOVE_LINK, SEAT_LINK, REPAIR, LOG_KEY_MESSAGE (added after this research) | Relay moves and the `VaultMoved` replay rule (`docs/relay-move.md` §7), seat links and recovery anchors, the sealed log key and create token |
| Unversioned | `pczt_hash` (must be the hash of the logged bytes, so parse → serialize must round-trip exactly), `commitments_hash`, the ZIP 2005 derivation (vectors exist), expiry rounding and the accepted window, the ZIP 317 exact-fee rule and the §9.3 check order, 512-byte memos |

## 6. Host wallets: how each would consume it

| Wallet | FFI | librustzcash vs ours | Licence | Fit |
|---|---|---|---|---|
| **Zodl Android** ([SDK](https://github.com/zodl-inc/zodl-android-wallet-sdk)) | Hand-written JNI cdylib (`backend-lib`) | **Identical lockfile** on 2026-10-01 (orchard 0.15.5, pczt 0.9.3, zcb 0.24.0, zcs 0.22.0, frost 3.0.0, reddsa 0.5.2), tonic 0.14, arti 0.35; Zafe has since moved to the NU7 pre-releases (orchard 0.16, pczt 0.10.0-pre.1, zcb 0.25.0-pre.1, zcs 0.23.0-pre.1) | **AGPL-3.0 + commercial** since 2026-08-24 (the MIT originals stay at `zcash/zcash-android-wallet-sdk`) | Best technical fit. It already has the Keystone flow: `importAccountByUfvk` with a null fingerprint gives `Spending { derivation: None }` (how we import), plus `createPcztFromProposal`, `redactPcztForSigner`, `addProofsToPczt` and `createTransactionFromPczt`. Our crates can link into their AGPL SDK; never copy their post-relicence code into ours. No public FROST plans |
| Zodl iOS ([SDK](https://github.com/zodl-inc/zodl-swift-wallet-sdk)) | cbindgen staticlib `libzcashlc` | Same, except orchard 0.15.4 | Same | Same flow (`zcashlc_*`) |
| **Vizor** ([repo](https://github.com/chainapsis/vizor-wallet)) | Flutter + FRB 2.11.1, one crate | **Moved to the `zakura-*` fork** (zakura-orchard 2.0.0, zakura-pczt rc4, zakura-reddsa 2.0.0, ff/group 0.14) | Apache-2.0 | Same app shape as Zafe, but needs a second build of our crates against Zakura (a Cargo rename feature plus a CI matrix entry). PCZT byte compatibility between our pczt and zakura-pczt is **unverified** |
| Zkool2 | Flutter + FRB 2.12 | ZSA git forks, frost **2.2**, its own sqlx DB, memo transport | MIT | Could take only the pure pieces (ZIP 2005 keys, verify), and only after moving to frost 3 |
| Zallet / zcash-devtool | Rust CLI | Compatible under `^` ranges | MIT/Apache | Zallet's multisig is transparent ZIP 48 ([zallet#872](https://github.com/zcash/zallet/issues/872)). devtool has the PCZT commands a CLI participant needs: a low-risk place for `zafe-protocol` as a CLI signer |
| Unstoppable, Edge | Old MIT Android SDK | — | — | Only via an SDK |
| Keystone, Ledger | — | — | — | No FROST. Keystone's PR #2133 is transparent multisig. A FROST share on Keystone would need QR round trips and on-device nonce storage, and the PCZT UR limit is about 14 KiB |

**How a host wallet uses a vault.** The vault looks like a Keystone account:
1. The wallet imports the UFVK as Spending with no derivation.
2. The wallet builds the PCZT with its own `createPcztFromProposal` (`OvkPolicy::Sender`,
   Ironwood change).
3. `zafe-coordinator` runs the proposal, approvals and signing, and returns the PCZT with
   `spend_auth_sig`s filled in.
4. The wallet proves (this can start in parallel) and broadcasts.

**Packaging for Zodl:** a Cargo feature inside *their* backend-lib that depends on our pure
crates and exports a few JNI / `zcashlc_*` functions. That keeps one orchard copy per
process. A second cdylib would not.

**Others building the same thing** (collaborate rather than duplicate):
- **Konclave** (deegalabs, Apache/MIT): browser + Tauri, blind relay, mainnet Ironwood,
  but its helper holds the viewing key.
- **aryaethn**: proven frost-tools contributor, ZCG #358 declined, planning a FROST backup ZIP.
- **pacu's FROST UniFFI SDK** (zecdev): reuse or align the mobile bindings.
- **Zafu/Zigner** (rotkonetworks): UniFFI, QR signer.
- **Zkool**.

**Flagship user: the lockbox key-holders** (ZF, ZODL, Shielded Labs).
- [draft-mcgee-keyholders-organizations](https://github.com/zcash/zips/blob/main/zips/draft-mcgee-keyholders-organizations.md)
  (2026-09-14) and ZIP 271 both expect the lockbox to move to a shielded, likely FROST, address.
- ZIP 270 (key rotation for tracked RedPallas keys "held via FROST") works at consensus
  level, not as a wallet API.

## 7. Risks and open issues

- **Fixed (2026-10-01): our `sk` agreement could be biased by the last contributor**
  (found while checking str4d's zips#895 comment against `keygen.rs`). Each member now
  commits to its contribution in DKG round 1, the commitments are part of the echo, and a
  reveal that doesn't match aborts keygen. The original analysis:
  - How it works today: `combine_vault_secret` hashes `vault_id ‖ transcript ‖ r_1 … r_n`.
    The `r_i` are sent sealed *after* the DKG, with no earlier commitment.
  - The problem: a member that waits for everyone else's `r_i` can grind its own to pick
    bits of `sk`.
  - Impact looks low, since that member learns `sk` anyway and can't spend alone. It is
    still a deviation reviewers will flag.
  - Fix: commit to `H(vault_id ‖ id ‖ r_i)` in DKG round 1 (bump `DKG_ROUND1`), reveal
    afterwards, and abort on a mismatch. Or adopt COCKTAIL's transcript binding when it
    ships.
- **ZF builds the core in-house** (COCKTAIL, the redpallas move, the devtool
  coordinator, #579/#580). We win with review, vectors and fixtures, not rival
  implementations. Expect larger PRs to be reshaped or redone.
- **ZF and ECC are slow on big changes** (zips#895 about 2 years, orchard#475 parked,
  devtool#137 open 7 months). Never block our releases on them.
- **Spec drift.** Our `sk` agreement differs from zips#895's COCKTAIL-payload design.
  Plan a versioned migration: new vaults use the new scheme and old ones keep working.
- **Crowded field.** A fourth app on its own won't become canonical. A spec co-authored
  with Konclave and aryaethn, and blessed by ZF, can.
- **Funders are wary of SDKs.** ZCG declined a wrapper SDK (2026-07-21), and ZF says
  "wallets should integrate the existing tools". So the pitch is not "an SDK" but the
  coordination protocol, the verifier and the vectors, already used by a shipping app and
  audited.
- **Zodl's licence.** After the AGPL relicence with a commercial option, an integration
  is a negotiation with ZODL, not just a PR.

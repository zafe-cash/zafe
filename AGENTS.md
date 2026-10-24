# AGENTS.md

Source of truth for agents working on Zafe. Read it before changing code; update it when you
learn something a later run would otherwise rediscover (a gotcha, an invariant, an upstream
change). **Keep entries short**: the rule, its reason in a clause, where it lives. History,
measurements and UI detail go in `docs/` (Flutter app, Android, art, site:
**`docs/app-notes.md`**). `CLAUDE.md` only contains `@AGENTS.md`.

Zafe is a Safe-style **shielded multisig for Zcash** (Ironwood pool) using re-randomized FROST.
Spec: `spec.md`; review: `spec-review.md`; open questions upstream: `upstream-asks.md`.
**Tracker: `docs/tracker.md`** (what's left, deferred, ideas): read it at the start of a task and
update it in the same change when you defer, discover or finish something. **Stack plan:
`docs/stack-plan.md`** (+ `docs/stack-design.md`): read before restructuring crates or
contributing upstream.

## Commands

```bash
cargo fmt --all && cargo clippy --workspace --all-targets     # must be clean
# CI uses latest stable with -D warnings; local default is pinned older (1.97):
CARGO_TARGET_DIR=target/stable-check cargo +stable clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# ZIP 2005 vectors: regenerate, then check independently in Python
ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors
python3 scripts/check_zip2005_vectors.py

# Live regtest (Docker + `ths`): in-process end-to-end spend
cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture
scripts/m0-e2e.sh                       # three `zafe` CLI processes through the relay
cargo test -p rust_lib_zafe --test bridge_e2e -- --ignored --nocapture   # app flow via the bridge

# Cross-check against frost-tools zcash-sign (build it from ZcashFoundation/frost-tools)
ZCASH_SIGN_BIN=/path/to/zcash-sign cargo test -p zafe-core --test zcash_sign_crosscheck -- --ignored
scripts/android-bench.sh                # phone benchmark over adb
cargo test -p zafe-core --test tls -- --include-ignored   # TLS clients + live testnet lightwalletd

# Relay image (repo root); use a spare port, never 8787 (emulator harness)
docker build -f infra/relay/Dockerfile -t zafe-relay:dev .
docker run -d --name zr -p 18899:8080 zafe-relay:dev && curl localhost:18899/health; docker rm -f zr
```

App (from `app/`, after `source ~/android/env.sh`): `flutter_rust_bridge_codegen generate` after
changing `app/rust/src/api`; `flutter analyze lib test` (must be clean; a fresh worktree needs
`dart pub get` in `rust_builder/cargokit/build_tool` before a plain `flutter analyze`);
`flutter build apk --debug --target-platform android-arm64`; `adb install -r
build/app/outputs/flutter-apk/app-debug.apk`. Screen previews: `flutter test tool/screens/<x>_render_test.dart`;
overflow check: `flutter test tool/screens/size_sweep_test.dart`.

Commits end with the attribution lines the harness gives. Never commit unless the user asked.

## Layout

```
crates/zafe-core    keys (ZIP 2005), keygen (DKG + sk), signing, tx (PCZT), verify (§9.3),
                    session, vault (descriptor, events, replay), wallet (zcash_client_sqlite),
                    relay_client, node (orchestration), log_cache, spend_watch, repair, sas, tor
crates/zafe-proto   identities, signed/HPKE envelopes, vault log, relay API types, `version`
                    (no Zcash deps, so the relay can use it)
crates/zafe-relay   blind axum relay on SQLite (ZAFE_RELAY_DB), push hook, pruning
crates/zafe-cli     `zafe` binary: headless member for tests (dev-only plain-file state)
app/                Flutter app; app/rust = bridge crate `rust_lib_zafe` (workspace member)
infra/regtest       regtest through `ths` (up.sh / fund.sh / down.sh)
infra/relay         Dockerfile, vps/ (Compose + Caddy + backups, bootstrap.sh, deploy.sh)
infra/site          website (Astro)
scripts/            m0-e2e.sh, android-bench.sh, app-harness.sh, brand/, illustrations/, sounds/
```

State dir names live in `zafe_core::state_dir`; CLI home files are consts at the top of
`zafe-cli/src/main.rs`. Never spell a path name inline.

## Protocol invariants (breaking these is a security or funds bug)

### Keys and setup
- **Vault keys (ZIP 2005, `use_qsk = true`)**: `ak` from the FROST DKG; the rest from the shared
  vault secret `sk`: `nk = ToBase(PRF^expand_sk[0x07])`, `qsk = trunc32(PRF^expand_sk[0x0C])`,
  `qk = BLAKE3.derive_key("Zcash ZIP 2005 qk-derivation v1", qsk)`,
  `rivk_ext = ToScalar(PRF^expand_qk[0x0D] || ak || nk)`; FVK via `FullViewingKey::from_bytes`.
  Never use frost-tools' `from_sk_ak_incompatible_with_quantum_recoverability...`.
- **DKG**: safety words confirmed out of band first; round-1 echo hashes must all match;
  round-2 packages and `sk` contributions are HPKE-sealed; every member signs the descriptor.
  Each `sk` contribution is committed in round 1 (`Round1Msg.sk_commitment`, part of the echo);
  a reveal that doesn't match aborts. `reddsa` 0.5.2's `post_dkg` normalizes `ak` to even Y.
- **Safety words** (`zafe_core::sas`): after sealing, `Kind::SafetyCommit`, then (only holding
  every other commitment) `Kind::SafetyReveal`; 4 BIP-39 words hash every key and value, so a
  relay gets one guess (2^-44). Secrets derive from an Ed25519 signature (nothing stored).
  Keygen takes the confirmed words and re-derives them. Seat recovery uses the same scheme
  (`repair::recovery_checks` / `new_phone_words`, relay `/v1/recovery/put|read`).
- **Keygen is a step function** (`node::keygen_step`; bridge `setup_step`; CLI/tests
  `node::run_keygen`, 1 step/s to stay under the hosted rate). State in `state_dir::KEYGEN`,
  sealed with the vault's `NonceKey`, **written before anything it covers is sent** (round 1
  is never generated twice; the rest is recomputed deterministically; receivers keep the first
  copy). Unreadable state is an error, never a fresh start. Created once via
  `durable::create_new` under a process-wide lock (**Android forbids hard links** in app
  storage). No new round 1 once the vault log has an entry. `KeygenProgress::Done` repeats
  until the caller saved the material and called `forget_keygen`. Tests: `tests/keygen_async.rs`.
- **Setup lifecycle on the relay**: the creator can cancel (`POST /v1/mailbox/cancel`) until
  the first log entry; invites admit **new** members for 72 h (`relay::INVITE_TTL_SECS`, 410 →
  `InviteExpired`), renewed by the creator (`/v1/mailbox/renew`) until sealed. Stalled waits
  fail with `NodeError::KeygenStalled { stage, missing }`. An invite is a bearer credential
  until membership is locked.

### Signing
- **Randomizer** = the PCZT's own `alpha` (fixed before round 1 because `rk` feeds the
  sighash). Deviates from ZIP 312 (accepted by ZF, frost#1094, and Daira-Emma Hopwood; spec
  §9.5.1). **`alpha` links `rk` to `ak`: it never leaves the members**; anything exporting a
  PCZT or signing package must strip it. Uses deprecated `frost_rerandomized::sign` in one wrapper.
- **What to sign**: every Ironwood action whose `spend_auth_sig` is `None`; never filter by
  value (zero-value vault spends exist). Reject any unsigned Orchard-pool spend.
- **Member verification** (`verify::verify_pczt`) before approving and again before signing:
  v6 + branch id + expiry; Ironwood only; **all spends checked before any output**; each spend
  is a vault note (nullifier/rk vs vault FVK) or a zero-value pre-signed dummy; each payment
  output recovered with the vault **external OVK** and matched exactly (recipient, amount,
  memo); change must belong to the vault **and trial-decrypt** with its IVK; fee must
  **equal** ZIP 317 (5000 × max(2, actions)); sighash computed locally.
- **Proposals use `OvkPolicy::Sender`** and Ironwood change, or verification fails. A vault can
  never pay its own addresses (`node::propose` refuses). Change is split into up to 4 notes
  (`wallet::change_split_policy`) while the vault has fewer than 4 spendable notes.
- **Shares are bound to the exact request** (request hash); aggregate only via
  `session::aggregate_request`. Signing requests are deterministic (same approvals → same
  bytes), so re-running `request_signatures` is safe.
- **Nonces**: one pair per spend, keyed by (proposal, pczt hash); check the request's packages
  against stored commitments **before** consuming; delete before sending a share; never reuse.
  Nonce, pool-nonce, kept-vote and leader files go through `zafe_core::durable` (temp + fsync +
  rename + fsync dir). Files are XChaCha20-Poly1305 encrypted with `NonceKey::derive(seeds,
  vault id)`; `put` errors so a failed write never publishes an approval. The directory must be
  excluded from backups/device transfer (Android done; iOS `isExcludedFromBackup` not yet).
  Interactive replies are kept (`keep_reply`) until the relay accepts them; `respond` skips
  closed/expired proposals.
- **Leader**: never messages itself (relay 403); when it is a signer it signs locally
  (`node::sign_own_shares`, keeps `<id>.own` until broadcast). Asks itself first, then members
  not in `SilentSigners`, then the silent ones (`node::order_signers`). Persists each request
  (`<id>.req`) and used commitment sets; **Start over** deletes only `.req`/`.own`, so the
  next round needs t fresh approvals. A member can re-approve exactly when it holds no nonces
  for (proposal, pczt hash). `used_commitments.bin` unreadable is an **error**, never empty.
- **Proving runs in parallel with share collection** (`node::finalize`): the proof doesn't
  depend on signatures. Proving keys are process-wide `OnceLock`s.
- **One-tap signing** (spec §9.5.2): members pre-publish commitments (`VaultEvent::Commitments`,
  nonces stored *before* appending); replaying a `Proposal` assigns each (group, spend, member)
  the member's next pool commitment, never reusing one, and only to groups whose **every**
  member has a pool (event v6+). **A vote with shares is final.** `sign_groups` checks every
  commitment is ours and present **before** taking any nonce. The first complete group is
  `ready_group`; anyone can send it (`send_ready` tries every complete group). Falls back to
  interactive when pools are short or C(n, t) > 64. Pools are published at the end of keygen,
  refilled below half by `top_up_pool` (run on every `list_proposals`), and drain when a
  proposal **enters the log**. A signed vote is kept (`PoolStore::keep_vote`) before appending
  and reposted on retry; never fall back to interactive for a member whose shares were lost
  (deadlocks a 2-of-2).
- **Restore vs one-tap**: backups never carry nonces, so a restore logs `VaultEvent::PoolReset`
  (retires the author's *unassigned* commitments; only on restore, via the `POOL_RESET` marker,
  CLI `zafe pool --reset`). Open proposals already assigned dead commitments need
  `node::propose_again` (`ProposalInfo.needs_new_proposal`).
- **Sweeps**: a `Proposal` with **no payments** spends a cancelled proposal's notes back to the
  vault so a signed cancelled tx can never mine. UI must handle `payments.isEmpty`. A
  proposal's `nullifiers` include padding spends: filter to the wallet's notes.

### Notes, expiry, sending
- **Note reservation** (spec §9.1). *Log rule* (`VaultState::apply`, deterministic): a
  `Proposal` spending a note of an earlier open/approved proposal with `expiry_height > tip`
  is ignored (`NotesInUse`); broadcast/cancelled ones don't block. Changing this changes
  replay: gate it like an event version. *Wallet holds* (`node::reserve_notes` →
  `VaultWallet::reserve`, upstream `OutputLockStore`): open, approved and broadcast proposals'
  notes are locked until expiry; a broadcast tx neither mined nor in lightwalletd's mempool
  releases them. **Read the whole mempool, never look up a txid** (privacy).
- **Sent transactions**: `finalize`/`send_ready` return `Sent { txid, raw }`; callers keep the
  raw bytes so `reserve_notes` can resend a dropped tx. A broadcast rejected by the node counts
  as sent when our txid is in the mempool (two senders race).
- **Expiry**: `descriptor.proposal_expiry_blocks` (default 7 days, creator picks 1-30); expiry =
  `vault::expiry_height(target, window)` rounded up to 144 blocks; members accept window + 96 +
  144. Never remove expiry. Counts are **75 s blocks**; from NU7 (25 s) `wallet::block_scale`
  = 3 scales window, rounding and slack. `wallet::proposal_expiry` cuts expiry to before a known
  upgrade height. Mainnet heights must come from `ZafeNetwork`, never `MAIN_NETWORK`.
- **Unapproved-spend alert** (`spend_watch`): a vault spend the log doesn't account for; grace
  only for Approved/Broadcast proposals and the txid their PCZT fixes. Never blocks anything.

### Vault log and versions
- **Replay is lenient after creation**: invalid entries go to `VaultState.ignored`
  (deterministic); only "Created first + all descriptor signatures" is fatal. Check
  `VaultState::check` before appending. Membership is **dynamic** (seat moves): use the
  replayed state (`members_by_pk(state)`, `frost_id_of(&state.descriptor, ..)`), never
  `material.descriptor`. Log chains accept entries signed by former members; replay ignores them.
- **Every format is versioned** (`zafe_proto::version`: one constant per format, leading LE
  `u16` tag). Decoders accept only known versions and fail with `UnsupportedVersion` (never
  garbage). Covered: envelopes and their payloads, log entries, vault events, descriptor, relay
  API bodies (HTTP 426 + `zafe-supported-version` → `UpdateRequired`/`RelayOutdated`), relay DB
  (`PRAGMA user_version`), invites, identity seeds, vault material, nonce files, leader files,
  backups, log cache. Unreadable nonce files = missing; `silent.bin` unreadable = empty.
  Not ours: FROST serializations, PCZTs, Zcash encodings, the wallet DB, app JSON caches.
- **Replay and versions**: an unknown-version event after `Created` is ignored and counted
  (`newer_version_entries()`); unknown version in `Created`/descriptor is fatal; an unknown log
  entry version stops catch-up. Once a v9 event is in the log, later events replay under the
  highest version seen.
- **Bumping a version**: change the constant; decode old tags via `version::split` where old
  data must stay readable. Log entries, events and descriptors live forever: keep every old
  decoder once external testers exist. New `VaultEvent` variants are **appended** (postcard
  index) with a `VAULT_EVENT` bump. **Members on different event versions diverge, so gate new
  event rules on every member having updated.** New relay routes need no `RELAY_API` bump (404
  = older relay, client falls back); bump only when an existing body changes. Record bumps here.
  - `VAULT_EVENT`: 2 `Name`, 3 `BackupVerified` (no gate); 4 `ReplaceApproval` (gate: all
    update before a seat move); 5 `RepairRetry`/`RepairDone`; 6 one-tap per fully covered group
    (gate by tag); 7 `Broadcast` must carry the PCZT's txid (gate); 8 `PoolReset` (gate);
    9 highest-version replay + parseable PCZT check (gate).
  - `BACKUP` 3 (2: signer names; 3: log copy; v1 and v2 still restore), `RELAY_DB` 4 (vault
    id + expiry columns; migrated from 1-3, old rows get `vault_id = id`, seen today, shared),
    `REPAIR` 3 (sigma carries the helper's `LogAnchor` + create token; v2 still read),
    `INVITE` 3 (2: safety words; 3: vault id + label + relay URL, v2 reads as label = vault
    id on the default relay), `VAULT_MATERIAL` 2 (`relay`; v1 reads as legacy),
    `NONCES`/`POOL_NONCE` 2 (encrypted; v1 still read), `DKG_ROUND1` 3, `LOG_CACHE` 1,
    `LOG_KEY_MESSAGE` 1 (setup log key + create token; the untagged 36-byte payload still
    reads), `SEAT_LINK` 1 (`zafe-seat-v1:`), `RELAY_STATUS` 1.

### Relay and client
- **Relay is blind**: it sees public keys, ciphertext, metadata. Clients drop envelopes for
  another mailbox, from non-members, badly signed, or with non-increasing seq.
- **Envelope seqs** (`node::SeqCounter`): microsecond clock but never below the last seq this
  process issued (clocks step back). Keygen, safety-word and signing-reply sends go through
  `node::send_fresh` (re-seal with a fresh seq on 409).
- **Rollback and loss** (spec §6.3, `log_cache`): each device keeps the verified log
  (`<dir>/<vault id>.log`, keyed by vault id, never the label; grows only). `load_log`
  rebuilds from it and refuses a relay missing our last entry (`RelayRolledBack`), with
  another one (`RelayForked`) or no mailbox (`RelayLostVault`). A copy that doesn't verify is
  `LogCopyInvalid`, never a silent first load. `log_cache::configure` once per process/isolate (bridge `identity()` fails closed
  otherwise). Restore with `reseed_relay`; leave a fork only via `node::follow_relay` (behind a
  dialog + unlock). Don't build anything that changes replay of existing entries without a gate.
- **Relay membership check**: `reseed` lets the restorer pick members, so
  `node::check_relay_membership` compares the relay's list with the replayed log (`RelayMembership`).
- **Relay hardening**: client reads at most 64 MiB per response; `log_append` verifies and
  charges **before** answering `Conflict`; joiners refuse a creator birthday above tip +
  leeway; `VaultKeys`'s `Debug` shows only `ak`.
- **Relay limits/quotas** (`zafe_relay::{limits, quota}`): off in `Relay::new()` (tests), on in
  the binary. **Charge a key only after its signature verifies.** Byte counters on `mailboxes`
  must change in the same transaction as every write/delete of `deliveries`/`log_entries`.
  IP limits need `into_make_service_with_connect_info`. New polling flows must stay under the
  hosted rate (300/min per key). No `--` comments in `SCHEMA` (breaks `DROP COLUMN`).
- **Long polls** (`POST /v1/wait`): bump the per-mailbox watch **after commit and after dropping
  the DB mutex**; waiters subscribe before reading and never hold the mutex. Unknown mailbox is
  403 here so 404 means "old relay". New write paths that should wake apps call
  `relay.waiters.signal(&mailbox)`.
- **Inbox ack** (`POST /v1/inbox/ack`): delete by cursor, never by range (an inbox mixes
  requests and leader shares); clients always read from cursor 0.
- **Labels** (`docs/relay-move.md` §4.6): a relay mailbox id is a label; `mailboxes.vault_id`
  (from the first entry) must match every entry's `EntryHeader.mailbox` (appends and reseed).
  `/v1/log/append` only reaches label == vault id; labelled vaults use `/v1/log/append-labelled`
  (`RelayClient::append_log_to`). Seat-move approvals are signed over the **vault id** (they
  are in the log too): labelled mailboxes use `/v1/mailbox/replace-labelled`
  (`replace_member_at`), which checks the stored vault id. New routes answer 404 only for
  "unknown mailbox" (`UNKNOWN_MAILBOX`): a bare 404 is an old relay (`RelayTooOld`).
- **Vault ids vs labels in clients** (`node::VaultIds`): the vault id binds keys, FROST ids,
  `sk` commitments, safety words, the descriptor and `EntryHeader.mailbox`/`Chain`/log copy;
  the **label** addresses the relay (envelope `Header.mailbox`, every signed request, appends,
  inbox, membership check). New vaults get a random label ≠ vault id (`create_vault`, which
  refuses a relay without `/v1/info`); legacy vaults have label = vault id. Never pass
  `descriptor.vault_id` to a `RelayClient` method: use `material.label()` / `ids()`.
- **Per-vault relay** lives in `VaultMaterial.relay` (`VaultRelay { url, label,
  create_token }`; secure storage, CLI `vault.bin`, backups). `url: None` = a pre-2026-10-25
  vault on the app-wide setting. Bridge calls build clients with `vault::vault_relay(default,
  &m)` / `invite_relay`; the `relay_url` Dart passes is only that fallback. The CLI always
  uses `--relay`. The create token reaches members only sealed (`LogKeyMessage` at setup,
  repair sigma) and in backups, never in invites or seat links; the creator's `setup_step`
  needs it on **every** call (it goes out once, with the log key).
- **Anchors**: a restore installs the backup's log copy (`node::install_log_copy`, verified)
  before the first relay load; a recovering phone checks every sigma's `LogAnchor` before
  trusting the log and saves its copy only after (`node::read_log_verified`). Relay
  reachability per vault: `LogCache::relay_status` (`<vault>.status`, written by `load_state`
  and the bridge watch, at most once a minute while reachable).
- **Expiry** (`zafe_relay::expiry`, off in `Relay::new()`): every authenticated mailbox
  request calls `relay.touch` after its membership check (writes at most once a day), except
  leaving (`/v1/push/forget`). Deletion goes through `delete_mailbox`: add any new
  per-mailbox table there. `GET /v1/info` is JSON so fields can be added without a bump.
- **Create token** (`ZAFE_RELAY_CREATE_TOKEN`): header `zafe-create-token`, checked only in
  `charge_creation` (create, reseed's create branch), after the per-IP creation bucket.
- **Performance**: the bridge pools relay (`RelayClient::pooled`) and lightwalletd
  (`wallet::connect_pooled`) connections; CLI/tests use `RelayClient::new` (pools are per
  runtime). Run independent requests together (`tokio::join!`, wrapped in `Box::pin` in deep
  async fns or rustc hits its query depth limit). Don't take the wallet lock across network reads.
- **No telemetry**, ever (`scripts/check-no-telemetry.sh`, CI): no analytics, crash-reporting or
  tracking SDK. Bugs go to the local scrubbed diagnostic log the user shares by hand. Log lines
  never include secrets.

### Wallet
- Import the vault UFVK as `AccountPurpose::Spending { derivation: None }` (`ViewOnly` can't
  spend). Birthday ≥ 2. `VaultWallet::create` does network calls before creating the DB file;
  `open` runs `init_wallet_db` (migrations).
- **Wallet DB is SQLCipher**: every connection goes through `wallet::open_connection` (raw key
  pragma, refuses if `cipher_version` is empty, `SQLITE_NOTADB` → `WrongKey`). The key is
  **derived**: `WalletKey::derive(identity_seeds, vault_id)`. Not in backups: a DB the key can't
  open is deleted and resynced. Access is serialized by the bridge's `wallet_lock()`.
- Don't add unmined receipts on top of `get_wallet_summary`: it already counts them once a sync
  recomputes it (0.1.20 double-counted).

## Dependency gotchas

- Exact pins live in `Cargo.toml` (spec §4.4): `reddsa =0.5.2` (`frost`; 0.6 removed FROST),
  `frost-core`/`frost-rerandomized` 3.0.0, `orchard =0.16.0`, and the NU7 pre-release line
  (`pczt =0.10.0-pre.1`, `zcash_client_backend =0.25.0-pre.1`, `zcash_client_sqlite
  =0.23.0-pre.1`). No stable librustzcash release supports NU7 yet.
- The Zcash APIs take `rand_core` 0.10 (`zafe_core::zrng`); our crypto stays on 0.6. `alpha`
  crosses pasta_curves 0.5 ↔ 0.6 through canonical bytes.
- `zcash_client_sqlite` needs `transparent-inputs` **and** `serde`; `zcash_keys`
  `unstable-frost` gives `UnifiedFullViewingKey::from_orchard_fvk`; `propose_transfer` /
  `create_pczt_from_proposal` need explicit error type params.
- Messaging crypto is the previous generation (`ed25519-dalek` 2, `hpke` 0.12,
  `chacha20poly1305` 0.10): newer needs `sha2` 0.11, which conflicts with `bip32`'s pin.
- `rusqlite` uses `bundled` workspace-wide; only `zafe-core` adds
  `bundled-sqlcipher-vendored-openssl` (needs `perl` + `make`). The relay DB stays unencrypted on
  purpose (public data only).
- `[profile.dev.package."*"] opt-level = 3` (Halo 2 is unusable unoptimized).
  `zcash_primitives/non-standard-fees` is a dev-dependency only.
- **TLS**: rustls + **ring** + **webpki-roots** everywhere; keep only one rustls provider.
  `tonic::Channel::from_shared("https://…")` does **not** enable TLS (`wallet::connect` sets
  `ClientTlsConfig`). Bundled roots don't see CA distrust: bump `webpki-roots` when a CA is pulled.
- **Tor** (`zafe_core::tor`, arti via `zcash_client_backend`): **fail-closed** process-wide
  policy; once `tor::request()` runs nothing connects directly. Choke points:
  `wallet::connect` and `RelayClient::exchange`; route every new network path through them.
  arti refuses local addresses (regtest can't use Tor). Full notes: `docs/app-notes.md`.
- `cargo build -p a -p b --examples` builds only examples. Killing `cargo` leaves `rustc`
  children running: stop builds with Ctrl-C/`TaskStop`; run one cargo at a time (`nice -n 15
  cargo ... -j 4`). Never `pkill -f` a pattern in your own command line.
- A `CARGO_TARGET_DIR` shared between worktrees races (same rlib names): use a private target
  dir when others build. Building `zafe-cli`/`zafe-relay` into `target/` replaces the binaries
  `scripts/app-harness.sh` runs.
- Disk: `target/` grows past 60 GB (delete `target/debug/incremental`); in CI a bus error or
  "No space left" in a link step means disk, not code.

## Infrastructure

- **Relay deploy**: `GET /health`; `PORT`/`ZAFE_RELAY_LISTEN`; FCM key from
  `ZAFE_FCM_SERVICE_ACCOUNT(_JSON)`. Docker context must contain every workspace member.
  One machine per SQLite file, never scale out.
- **Hosted relays**: testnet `testnet.relay.zafe.cash`, mainnet `relay.zafe.cash` (same VPS,
  compose profiles). **Never deploy by hand**: testnet deploys on push to `main`; mainnet only
  via `relay-mainnet.yml` (promotes a soaked testnet digest, reviewers required). Separate
  forced-command deploy users per network (`vps/zafe-deploy`, change it via `bootstrap.sh`).
  DNS must stay **DNS only** in Cloudflare. Publish no Docker port but Caddy's (bypasses ufw).
  Compose variables need defaults. Details: `infra/relay/README.md`, `docs/audit-scope.md` §10.
- **Promote the relay before an app that needs it ships** (e.g. FCM FIDs, `PushPlatform::FcmFid`).
- Public lightwalletd: testnet `https://testnet.zec.rocks:443`, mainnet `https://zec.rocks:443`.

## Regtest (infra/regtest)

- Regtest is `ths` (thus-spoke-zakura), pinned `THS_VERSION` in `up.sh` (0.4.0: Zakura 1.6.0,
  NU5..NU7 at height 1); keep it on the latest release. Install:
  `curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/zcashlabs/thus-spoke-zakura/main/install.sh | THS_VERSION=0.4.0 sh`.
- `up.sh` runs ths under `setsid nohup` (pid + log in `~/.cache/zafe-regtest/`). Ports: RPC
  18232, lightwalletd 9067, + offset (multiple of 10). Offsets: harness 0, `regtest_e2e`
  30000, `bridge_e2e` 30100, `m0-e2e.sh` 30200. `down.sh` stops it.
- Fund with `fund.sh <ua> [notes]` (5 ZEC each, + 12 blocks); one note per concurrently open
  proposal. Mine with `ths --name <name> mine N`. Every start is a fresh chain
  (`app-harness.sh resume` refunds). Restarting the node rewinds the last ~100 blocks.

## App essentials (details: `docs/app-notes.md`)

- Flutter (fvm, **3.47.7**) + `flutter_rust_bridge` 2.11.1 + Riverpod + go_router, built on
  Vizor (chainapsis/vizor-wallet, Apache-2.0) with **Zafe's own look**: keep attribution in
  `NOTICE`; no Vizor references in code comments; never copy Vizor's art, name or IDs.
- FRB: write `Result<T, ZafeError>` in public signatures (a type alias becomes anyhow and drops
  the typed error). Streams report failures as events, not `Err`.
- New in-vault routes go in the `inVault` list in `app.dart`'s redirect. Switch tabs with
  `context.go`, never `push`.
- Secure storage only through `ZafeSecureStore` (`_PatientStorage`): plugin calls can hang on
  Android while a background engine uses it. Endpoints only via `endpointsProvider` /
  `ZafeEndpoints.fromPrefs` (after `prefs.reload()`), never the build consts.
- **Relays in Dart**: `endpointsProvider.relayUrl` is only the default for new vaults (and
  the fallback the bridge uses for pre-per-vault vaults); show a vault's relay from
  `activeVaultRelayProvider`. The creator's create token lives in `ZafeSecureStore` with
  the invite until the material is saved; `stepSetup` passes it on every step. The relay
  picker checks only the selected relay, never the list.
- Never drop a refresh request because work is running (`CoalescedRun`, `refreshSoon`).
- Gated actions call `confirmUnlock` first; background checks act only on approvals already given.
- UX rules (user): minimal text, exact values, no toasts (`ActionButton` + `appNoticesProvider`),
  no dead ends; check 360x640 at 1.0 and 2.0 text scale and both proposer and approver flows.
- `pubspec.yaml` has a single `flutter:` key; font names match it exactly.
- `shared_preferences_android` pinned to 2.4.23 (2.5 breaks minified release builds).
  Release APKs: always `--split-per-abi` with one `--target-platform`.
- Testnet builds are a separate Android app (`xyz.zafe.zafe.testnet`); the harness and adb
  commands use `xyz.zafe.zafe`. Reset test devices with `adb shell pm clear xyz.zafe.zafe`.
- Device testing: emulator AVD `zafe` (`emulator -avd zafe -gpu swangle_indirect -no-snapshot
  -no-audio`, never swiftshader); `scripts/app-harness.sh` runs relay + CLI members B and C
  (state in `~/.cache/zafe-harness`; `cargo build -p zafe-cli` after core changes). WSL2 has no
  USB: `adb pair IP:PORT CODE` on one line. Record with `adb shell screenrecord` as a
  background task. Phone: proof ~1.7 s on 8 threads, round 2 12 ms / 6.8 MB.
- Toolchain: `~/android/install-toolchain.sh`, `source ~/android/env.sh`. env.sh puts
  `~/flutter` (3.41, too old for `pubspec.yaml`) first: then `export
  PATH=$HOME/flutter-3.47/bin:$PATH` (no `fvm` binary here).

## Upstream status (check before relying on it)

- **NU7**: active on testnet since 4,465,026 (branch id `0x77190AD9`); mainnet at
  `MAINNET_NU7_HEIGHT` = 3,543,405 (~2026-11-05; supplied by `wallet::ZafeNetwork::Main` until
  a crate release has it). Guards: `wallet::ensure_current_branch` in `node::propose` (→
  `UpdateRequired`; sync still works), branch-id broadcast rejections map to the same error,
  expiry cut before the upgrade. v6 transactions stay valid; the ZIP 233 fee change is behind
  an unstable flag, so the ZIP 317 check is unchanged.
- **Cleared with ZF** (frost#1094, 2026-10-01): U1 (own `sk` agreement), U5 (builder-chosen
  `alpha`, kept secret), U3 (redpallas moves to the FROST repo). Mainnet beta waits on the audit
  caveat in README/site/release notes.
- Contributing to frost-tools: fork `zafe-cash/frost-tools`; Conventional Commit PR titles;
  CI = fmt, clippy -D warnings, test.
- Post-quantum: every member holds `qsk`, so one member's `qsk` suffices (ZIP 2005; spec §2.3).
- COCKTAIL-DKG (frost#1033) not ready; align when it ships with Pallas. RedPallas FROST moving
  out of `reddsa` (frost#963): stay on 0.5.2. Changing t (frost#1082): resharing prototype in
  `tests/reshare.rs`, not shipped.

## Machine hygiene

- **Never run `docker system/volume/image prune`**: other projects' data live here. Remove only
  what you created, by name.
- After a WSL restart run `scripts/app-harness.sh resume`.

## Fixing a bug

Learned from the 0.1.20 double count (a diagnosis from our own stale comment shipped a fix that
counted a payment twice):
- Write down at least two causes that fit what you saw, and find the observation that tells
  them apart.
- A dependency's behaviour comes from its pinned source (`~/.cargo/registry/src/*/<crate>-<ver>/`,
  `~/.pub-cache/hosted/pub.dev/<pkg>-<ver>/`) or a call to it, never from names, our comments or
  memory. Quote file:line in the commit.
- Test the outcome the user sees through the real dependency, and make the test fail on the
  broken code first.
- See it work (phone, emulator or harness) before tagging a user-facing fix.
- Fix wrong comments in the same change.

## User preferences

- Research primary sources (repos, ZIPs, issues) before asking or relying on memory; Zcash
  moves fast.
- Record durable learnings as you go: rules here, detail in `docs/`.

# AGENTS.md

Source of truth for agents working on Zafe. Read this before changing code; update it when
you learn something a later run would otherwise have to rediscover (a gotcha, an invariant,
an upstream status change). `CLAUDE.md` only contains `@AGENTS.md`.

Zafe is a Safe-style **shielded multisig for Zcash** (Ironwood pool, NU6.3) using
re-randomized FROST. Product spec: `spec.md`. Original review: `spec-review.md`. Open
questions to the Zcash Foundation / others: `upstream-asks.md`.
**Tracker: `docs/tracker.md`** lists what's left, deferred items and ideas.
**Stack plan: `docs/stack-plan.md`** (+ research in `docs/stack-design.md`): making the
protocol crates and spec the shared multisig layer for other wallets; read it before
restructuring crates or contributing upstream. Read it at the
start of a task; when you defer something, discover a gap or finish an item, update it in
the same change.

## Commands

```bash
cargo fmt --all && cargo clippy --workspace --all-targets     # must be clean
cargo test --workspace                                        # ~68 tests, 3 ignored

# ZIP 2005 vectors: regenerate, then check independently in Python
ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors
python3 scripts/check_zip2005_vectors.py

# Live Ironwood regtest (Docker + `ths`, see Regtest below): in-process end-to-end spend
cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture
# M0 acceptance: three separate `zafe` CLI processes through the relay on regtest
scripts/m0-e2e.sh

# Cross-check against frost-tools zcash-sign (build it from ZcashFoundation/frost-tools)
ZCASH_SIGN_BIN=/path/to/zcash-sign cargo test -p zafe-core --test zcash_sign_crosscheck -- --ignored

# Phone benchmark (proving + round-2 signing) over adb
scripts/android-bench.sh

# The app's payment flow through the Flutter bridge API (3 members, regtest, Docker)
cargo test -p rust_lib_zafe --test bridge_e2e -- --ignored --nocapture

# TLS clients (local rustls server) + live public testnet lightwalletd over TLS
cargo test -p zafe-core --test tls -- --include-ignored

# Relay image (from the repo root); run on a spare port, never 8787 (emulator harness)
docker build -f infra/relay/Dockerfile -t zafe-relay:dev .
docker run -d --name zr -p 18899:8080 zafe-relay:dev && curl localhost:18899/health; docker rm -f zr
```

Commits end with the attribution lines the harness gives (Co-Authored-By, and
Claude-Session when present). Never commit unless the user asked for the work.

## Layout

```
crates/zafe-core    keys (ZIP 2005), keygen (DKG + sk), signing, tx (PCZT), verify (§9.3),
                    session (approve/sign/leader), vault (descriptor, events, replay),
                    wallet (zcash_client_backend/sqlite), relay_client, node (orchestration),
                    log_cache (this device's copy of each vault log), spend_watch
                    (unapproved-spend alert)
crates/zafe-proto   identities, signed/HPKE envelopes, vault log, relay API types
                    (no Zcash deps, so the relay can use it)
crates/zafe-relay   blind axum relay on SQLite (ZAFE_RELAY_DB), push hook, pruning
crates/zafe-cli     `zafe` binary: headless member for tests (dev-only plain-file state)
infra/regtest       regtest through `ths` with NU6.3 active (up.sh / fund.sh / down.sh)
infra/relay         relay Dockerfile, fly.toml template, vps/ (Docker Compose + Caddy +
                    backup sidecar, bootstrap.sh, deploy.sh); hosted testnet relay deployed
                    by .github/workflows/relay-deploy.yml (README Path B)
scripts/            m0-e2e.sh, android-bench.sh, check_zip2005_vectors.py
```

## Protocol invariants (breaking any of these is a security or funds bug)

- **Vault keys (ZIP 2005, `use_qsk = true`)**: `ak` from the FROST DKG; everything else from
  the shared vault secret `sk`: `nk = ToBase(PRF^expand_sk[0x07])`,
  `qsk = trunc32(PRF^expand_sk[0x0C])`, `qk = BLAKE3.derive_key("Zcash ZIP 2005 qk-derivation v1", qsk)`,
  `rivk_ext = ToScalar(PRF^expand_qk[0x0D] || ak || nk)`. FVK via `FullViewingKey::from_bytes`.
  Never use frost-tools' `from_sk_ak_incompatible_with_quantum_recoverability...` (not recoverable).
- **DKG**: safety number confirmed out of band before starting; round-1 echo hashes must all
  match; round-2 packages and `sk` contributions are HPKE-sealed; every member signs the
  descriptor. Each `sk` contribution is committed in round 1 (`SkContribution::commitment`,
  in `Round1Msg.sk_commitment`), the commitments are part of the echo every member
  compares (`echo_with_commitments`), and a revealed `r_j` that doesn't match aborts
  keygen: no member can choose its contribution after seeing the others'. `reddsa`
  0.5.2's `post_dkg` normalizes `ak` to even Y (orchard rejects odd).
- **Randomizer**: the FROST randomizer for each spend is the PCZT's own `alpha` (fixed before
  round 1 because `rk` feeds the sighash). Secure per the Re-Randomized FROST paper; matches
  frost-tools' `zcash-sign`. **Deviates from ZIP 312**, which says the Coordinator MUST
  derive the randomizer after round 1 from fresh bytes + the commitment list (a hedge, per
  its rationale); asked as U5, spec §9.5.1. Daira-Emma Hopwood (Discord, 2026-10-01): the proof doesn't
  need `alpha` after the commitments and even lets the adversary pick it, **but `alpha`
  links `rk` to `ak`**: it must never leave the members (it lives only in the PCZT: encrypted
  log, HPKE requests, devices). Anything that exports a PCZT or signing package outside
  the vault must strip `alpha`. Uses the deprecated
  `frost_rerandomized::sign` in one wrapper (frost#1094: an external-randomizer API stays).
  ZF accepts the deviation (conradoplg, frost#1094, 2026-10-01: the step only hedges a weak
  coordinator RNG; everyone must keep `alpha` secret anyway).
- **What to sign**: every Ironwood action whose `spend_auth_sig` is `None` — never filter by
  value (zero-value vault spends exist). True dummy spends are already signed by the IO
  Finalizer. Reject any unsigned Orchard-pool spend (vaults never hold Orchard funds, ZIP 326).
- **Member verification** (`verify::verify_pczt`) before approving and again before signing:
  v6 + branch id + expiry; Ironwood only; **all spends checked before any output** (stable
  errors); each spend is a vault note (nullifier/rk vs vault FVK) or a zero-value pre-signed
  dummy; each payment output recovered with the vault **external OVK** and matched exactly
  (recipient, amount, memo); change must belong to the vault **and trial-decrypt** with its IVK;
  fee must **equal** ZIP 317 (5000 × max(2, actions)); sighash computed locally.
- **Proposals are built with `OvkPolicy::Sender`** and Ironwood change, or verification fails.
- **Note reservation** (spec §9.1): two layers.
  *Log rule* (`VaultState::apply`, deterministic): a `Proposal` whose PCZT spends a note
  of an earlier **open/approved** proposal with `expiry_height > new.tip_height` is
  ignored (`VaultError::NotesInUse`). Broadcast and cancelled proposals don't block (a
  respend is how you invalidate; at most one tx mines). `ProposalState` carries the
  parsed `nullifiers` + `expiry_height` (empty/0 when the PCZT doesn't parse; test
  fixtures use fake PCZTs). Changing this rule changes replay: gate it like an event
  version once external testers exist.
  *Wallet holds* (`node::reserve_notes` → `note_holds` → `VaultWallet::reserve`): open,
  approved and broadcast proposals' notes are locked with upstream `OutputLockStore`
  until expiry (owner = PCZT hash; `reserve` clears and re-locks, mirroring the log). A
  broadcast tx that's neither mined (`tx_mined`) nor in lightwalletd's mempool
  (`wallet::mempool_txids`, whole mempool, never a txid lookup: privacy) releases its
  notes, unless the chain moved past the wallet meanwhile. Run by `node::propose` (which
  now takes a lightwalletd client and rebuilds up to 3× on `NotesInUse`) and by the
  bridge's `sync_vault` (takes `relay_url` + `seeds`; best effort, locks persist in the
  DB). Locked notes leave `spendable_value`, stay in `total`. Wallet errors from
  `node::propose` are `NodeError::Wallet`, so `InsufficientFunds` / `FundsReserved` stay
  typed; `append_event`'s check failure is `NodeError::Invalid(VaultError)`.
- **Sent transactions** (`node::SentTxs`): `finalize` / `send_ready` return `Sent { txid,
  raw }`; callers keep the raw bytes (bridge `sent_txs(db_dir, m)`, CLI `<home>/sent`) so
  `reserve_notes` can resend a transaction that dropped out of the mempool. Regtest
  gotcha: Zakura (like Zebra) keeps the last ~100 blocks in memory, so restarting the
  node rewinds the chain as well as the mempool; don't use it to simulate a drop.
- **Nonce storage**: `nonce_store::FileNonceStore` (atomic write+rename; `put` returns an
  error so a failed write never publishes an approval). The directory must be excluded from
  backups/device transfer: the Android app disables both (`allowBackup=false`,
  `res/xml/data_extraction_rules.xml`); iOS needs `isExcludedFromBackup` (not done yet).
- **Nonces**: one pair per spend, keyed by (proposal, pczt hash); check the request's
  packages against stored commitments **before** consuming; delete before sending a share;
  never reuse. Re-approval produces fresh commitments; leaders track used commitment sets.
- **Proving runs in parallel with share collection** (`node::finalize`, Vizor's Keystone
  pattern): the Halo 2 proof doesn't depend on spend-auth signatures, so the unsigned PCZT
  is proved on a blocking thread, then signatures are applied to the proved PCZT (verified
  on regtest). Proving/verifying keys are process-wide `OnceLock`s (`node::proving_key()`).
- **The leader never messages itself**: the relay rejects self-addressed envelopes (403).
  When the leader is one of the chosen signers, `request_signatures` skips it and the
  leader signs locally with `node::sign_own_shares`, keeping the serialized shares
  (`<id>.own`) until broadcast, because signing consumes its nonces; `finalize` takes them
  as `own_shares`. M0/regtest tests originally missed this (their leader never approved);
  `bridge_e2e` now makes the leader an approver.
- **Signing requests are deterministic** (same approvals → same bytes → same request
  hash), so re-running `request_signatures` after a partial failure is safe.
- **Leader resumability**: the app persists each signing request (`<state>/leader/<id>.req`)
  and the used-commitments set; `send_proposal` after a timeout resumes the same round.
  **Start over** (`restart_signing`) deletes `<id>.req` and `<id>.own` only; the old
  round's commitment sets stay in `used_commitments.bin`, so the next request needs t
  approvals with fresh commitments. A member can approve again exactly when its nonce
  store has no nonces for (proposal, pczt hash) (`Member::approve` refuses otherwise);
  the bridge exposes that as `ProposalInfo.needs_reapproval`. An unresponsive signer
  keeps unusable nonces (never reused). `send_with_progress` takes the share-collection
  timeout (tests use 5 s; the app 90 s). Expiry is computed in Dart from the synced tip
  (`proposalExpired`: open/approved and tip ≥ `expiry_height`).
- **One-tap signing** (spec §9.5.2; `vault::assign_commitments`, `Member::sign_groups`,
  `node::{approve, send_ready, top_up_pool, forget_closed}`): members pre-publish
  commitments (`VaultEvent::Commitments`, nonces stored *before* appending); replaying a
  `Proposal` deterministically assigns each (group, spend, member) the member's next pool
  commitment, never reusing one; approving signs every group containing the member and
  posts the shares in the `Vote`; **a vote with shares is final**; the first complete
  group is `ready_group` and anyone aggregates it from the log (`send_ready`), the member
  who completed it (`completed_by`) auto-sends when `auto_send`. `sign_groups` checks every
  commitment is ours and present **before** taking any nonce. Falls back to interactive
  signing when pools are short or C(n, t) > 64. Pool size (`node::pool_target`):
  `POOL_PROPOSALS` = 16 single-spend proposals' worth, C(n-1, t-1) each, at least
  `MIN_POOL_TARGET` = 32 and at most one batch (256; replay caps a member at 4096
  outstanding); published first at the end of keygen (bridge `run_keygen` takes the vault's
  `state_dir`; CLI `vault keygen`), while every member is present, so the first payment
  is one tap (it used to wait for each member's first refresh; the harness's CLI members
  never published, and the app fell back to interactive); `assign_commitments` assigns a
  signer group only when **all of its members** have pool commitments (event version 6,
  2026-10-07; before it, one member without a pool made the whole proposal interactive,
  and proposals written with versions 1-5 still replay that way: the rule is keyed by the
  version of the author's app, `VaultState::apply_versioned`). A member in no assigned
  group approves interactively; refilled when below half by `top_up_pool`, which the bridge's
  `list_proposals` runs on every refresh (app poll, after approving/proposing, and each
  background check). Pools drain when a proposal **enters the log**, for every member,
  approving or not. Security reading of ePrint 2024/436 is in
  spec §9.5.1; confirmed by Daira-Emma Hopwood and conradoplg 2026-10-01 (U5).
- **Sweeps**: a `Proposal` with **no payments** spends a cancelled proposal's notes back
  to the vault (`node::invalidate`, `VaultWallet::propose_sweep`) so a fully signed
  cancelled transaction can never be mined. UI code must handle `payments.isEmpty`
  (`SweepCard`, rows say "To vault"). A proposal's `nullifiers` include padding spends
  that match no note: filter to the wallet's notes before requiring them.
- **Expiry**: `descriptor.proposal_expiry_blocks` (default 7 days; the creator picks 1-30
  days, sent in its DKG round-1 message, `DKG_ROUND1` = 3 since the `sk` commitment); proposer sets expiry =
  `vault::expiry_height(target, window)` (rounded up to 144 blocks so it doesn't date the
  proposal); members accept window + 96 + 144 blocks. Never remove expiry: a
  complete one-tap group stays sendable until it.
- **Every format is versioned** (`zafe_proto::version`: one constant per format, the
  `Format` enum, `encode`/`decode` for postcard and `frame`/`unframe` for raw bytes; the
  tag is a leading little-endian `u16`). Decoders accept only the current version and fail
  with `UnsupportedVersion { format, found, supported }` (`is_newer()` = "update the
  app"), never with garbage. Inventory:
  - **Envelope** (`ENVELOPE`): `Envelope::to_bytes` tag + `Header.version` (signed, HPKE
    AAD). It also covers the raw payloads (DKG echo hash, round-2 packages, `sk`
    contributions, descriptor signatures, log key). Postcard payloads have their own tag:
    `DKG_ROUND1`, `SIGNING_REQUEST` (also the leader's `<id>.req`; the request hash covers
    the tag), `SIGNATURE_SHARES`.
  - **Log entry** (`LOG_ENTRY`): `LogEntry::to_bytes` tag + `EntryHeader.version` (signed,
    AEAD AAD, in the entry hash). The relay stores and serves exactly these bytes.
  - **Vault event** (`VAULT_EVENT`): tagged plaintext of each entry. **Descriptor**
    (`DESCRIPTOR`): the `version` field inside the hash every member signs; checked at
    replay and when loading material.
  - **Relay API** (`RELAY_API`): tagged request/response bodies; `Signed<T>` signs the
    version. Inbox/log responses carry each envelope/entry as its own tagged bytes.
    A body in a version the relay doesn't speak gets **HTTP 426** with header
    `zafe-supported-version`; the client turns it into `RelayClientError::VersionRejected`
    → bridge `ZafeErrorKind::UpdateRequired` (app older) or `RelayOutdated` (relay older).
    New endpoints add new body types under the same tag without a bump (no existing body
    changes); a client meets an older relay as HTTP 404 on the new route and falls back
    (e.g. `/v1/wait` → `Ok(None)` → the app keeps polling). Bump `RELAY_API` only when an
    existing body changes. Relay DB: `PRAGMA user_version` = `RELAY_DB`; a newer DB, or one from before
    versioning (tables but version 0), is refused at startup: delete it. Bumps:
    **2** (2026-09-30) added the quota counters `mailboxes.delivery_bytes/log_bytes`;
    a schema-1 DB is migrated at startup (`migrate_from_v1`: ALTER + backfill).
  - **Device state**: invites (`zafe-invite-v1:`, `INVITE`; another version →
    `UnsupportedVersion`, not "bad invite"), identity seeds (`IdentitySeeds::to_bytes`,
    66 bytes: secure storage, CLI `identity.bin`, backups), vault material
    (`VaultMaterial::to_bytes`: secure storage, CLI `vault.bin`, backups), nonce and pool
    nonce files (an unreadable version counts as missing: never used), leader `.own`
    (`OWN_SHARES`) and `used_commitments.bin` (`USED_COMMITMENTS`; unreadable is an
    **error**, never "empty", or a commitment set could be reused), backups (`ZAFEBAK`
    byte = `BACKUP`, text `zafe-backup-v1:`; the text prefix is not the format version),
    the log copy (`log_cache`, `<vault id hex>.log`, `LOG_CACHE`: an unparsable file is
    "no anchor", a newer version is an error).
  - Not ours to version: FROST serializations (frost-core header with ciphersuite id),
    PCZTs (own magic + version), Zcash encodings (UFVK, addresses, memos), the wallet DB
    (zcash_client_sqlite migrations), the app's JSON caches (`seen.json`,
    `summary.json`; rebuilt by the app).
- **Replay and versions**: a `VaultEvent` in an unknown version after `Created` goes to
  `VaultState.ignored` like any invalid entry (`newer_version_entries()` counts newer
  ones, so the app can ask to update; not shown in the UI yet); an unknown version in the
  `Created` event or its descriptor is fatal. A log entry in an unknown version stops
  catch-up with `UnsupportedVersion` (the chain can't be verified past it). Inbox
  envelopes in unknown versions are dropped like badly signed ones.
- **Bumping a version**: change the constant in `zafe_proto::version`; where old data must
  stay readable, decode the old tag via `version::split` and migrate. Log entries, events
  and descriptors live forever in the vault log, so once external testers exist their
  decoders must keep every old version. Adding a `VaultEvent` variant: append it (old
  variants keep their postcard index, so old bytes still decode) and bump `VAULT_EVENT`
  so older members record `UnsupportedVersion` (and can prompt an update) rather than a
  generic decode error. Members on different event versions reach different states until
  they update, so gate new event types on every member having updated. Record each bump
  here. Bumps so far: `BACKUP` 1 → 2 (2026-09-30, signer names added to the contents;
  the header byte is the tag, not `version::split`, so `backup::decrypt` matches byte 1
  itself and migrates `ContentsV1` with no names; tested in `backup::tests`).
  `VAULT_EVENT` 1 → 2 (2026-10-01, `VaultEvent::Name`): `VaultEvent::from_bytes` accepts
  every tag from 1 to the current one (v2 only appended a variant), so v1 logs replay
  unchanged (`version_1_events_still_replay`). An older app skips `Name` entries and
  counts them as newer; nothing but names depends on them, so no gate was needed.
  `VAULT_EVENT` 2 → 3 (2026-10-01, `VaultEvent::BackupVerified`): appended the same way;
  an older app skips attestations (backup health only), so no gate either.
  `VAULT_EVENT` 3 → 4 (2026-10-01, `VaultEvent::ReplaceApproval`, seat moves). **Gate:** an
  older app ignores the approvals, never moves the seat, and then rejects the new key's
  log entries as a non-member's: every member must update before anyone moves a seat.
  `RELAY_DB` 2 → 3 (`mailboxes.threshold`, migrated from 1 and 2). New formats `REPAIR`
  (delta/sigma payloads) and `RECOVERY_REQUEST` (`zafe-recover-v1:`); new envelope kinds
  `RepairDelta`/`RepairSigma` were appended (older apps drop envelopes they can't decode).
  `VAULT_EVENT` 4 → 5 (2026-10-01, `RepairRetry`, `RepairDone`) and `REPAIR` 1 → 2 (deltas
  and sigmas carry the attempt); same gate as 4.
  `VAULT_EVENT` 5 → 6 (2026-10-07, no new variant): proposals written with version 6 get
  one-tap commitments per fully covered signer group (see "One-tap signing"). **Gate by
  construction**: the rule applies to events *tagged* 6, so an app that doesn't know 6
  skips them (`newer_version_entries`, "Update Zafe") instead of computing another
  assignment, and old logs replay as before. Cost: every event a version-6 app writes
  (votes, names too) is invisible to older apps, so members must update together.
  `LOG_CACHE` 1 (new, device file). `RELAY_API` unchanged: `POST /v1/mailbox/reseed`
  is a new route (404 on an older relay) and 507 now carries `zafe-quota: <token>`.
  `VAULT_EVENT` 6 → 7 (2026-10-07, no new variant): a `Broadcast` written with version 7
  must carry the txid the proposal's PCZT fixes (`ProposalState.expected_txid` = its
  shielded sighash; v6 txids exclude signatures and proof), else `TxidMismatch` and it is
  ignored. **Gate:** an older app skips every version 7 event, so all members must update
  together before anyone sends. Versions 1-6 broadcasts replay unchecked, forever.
  **Pre-release: nothing reads the unversioned bytes from before 2026-09-30**; reset
  test devices (`adb shell pm clear xyz.zafe.zafe`), the harness
  (`scripts/app-harness.sh stop`) and relay DBs after pulling this change.
- **Shares are bound to the exact request** (request hash); aggregation always goes through
  `session::aggregate_request` (signer-set check + per-share verification).
- **Vault log replay is lenient after creation**: invalid entries go to `VaultState.ignored`
  (deterministic across members); only "Created first + all descriptor signatures" is fatal.
  Check `VaultState::check` before appending.
- **Wallet**: import the vault UFVK as `AccountPurpose::Spending { derivation: None }`
  (Keystone pattern). `ViewOnly` accounts don't track witnesses and can never spend.
  Birthday height must be ≥ 2 (sync fetches tree state at start−1; lightwalletd treats 0 as unset).
- **Wallet DB is encrypted (SQLCipher, spec §14)**: every connection goes through
  `wallet::open_connection`, which runs `PRAGMA key = "x'<64 hex>'"` (raw 256-bit key, no
  KDF) first, refuses to continue if `PRAGMA cipher_version` is empty (SQLCipher missing
  would silently ignore the key) and maps `SQLITE_NOTADB` to `WalletError::WrongKey`.
  `WalletDb::for_path` can't take a key, so `open_wallet_db` opens the connection itself,
  loads `rusqlite::vtab::array` (what `for_path` does) and uses `WalletDb::from_connection`.
  The received-payments read-only connection takes the same key. The key (`WalletKey`) is
  32 random bytes per vault in `ZafeSecureStore` (`zafe_vault_<id>_walletKey`, made on
  first use by `walletKey(id)`), passed to every wallet bridge call as `dbKey`. **Not in
  backups**: the wallet is a chain-data cache; a DB the key can't open (plain DB from before
  encryption, lost key, another isolate's racing key) is deleted by the bridge's
  `open_wallet` and resynced from the birthday; `list_received` reads it as empty until
  then. No `sqlcipher_export` migration on purpose. The CLI keeps a dev key in
  `<home>/wallet.key` (plain file, like the rest of its state). SQLCipher logs key
  failures to stderr/logcat ("hmac check failed for pgno=1"): expected on a wrong key.
- **No telemetry** (decided 2026-10-07; Zodl is opt-in/Apple-only crash reports, Vizor has none): no analytics,
  funnel, crash-reporting or tracking SDK, ever; privacy beats telemetry. `scripts/check-no-telemetry.sh`
  (CI job `no-telemetry`) fails on such a package in pubspec/Gradle/Podfile/Cargo/site lockfiles. Bugs: the
  **local diagnostic log** (`core/diagnostics`: `CrashLog` hooks `FlutterError.onError` and
  `PlatformDispatcher.onError` in `main()`, `scrubDiagnostics` strips addresses, hex, links, amounts, vault
  ids and app paths before writing, 20 entries / 64 KiB in `<support>/diagnostics/crashes.log`); Settings >
  Privacy > "Diagnostic report" shows it in full and the user shares it by hand. Each isolate appends
  **only to its own file** (`CrashLog.forDir`: app `crashes.log`, WorkManager/FCM engine `crashes-bg.log`,
  installed by `installBackgroundCrashLog()` at the start of `checkVaultAndNotify`; routine offline sync
  failures are not logged, so they can't evict real errors) and `read()` merges both by timestamp; clear
  deletes all. Rust panics: `app/rust/src/diag.rs` (not in `api/`, so no FRB surface) installs a hook from
  `init_app` writing `<ts> panic at file:line:col` (never the message) to `diagnostics/rust-panics.log`;
  the directory is learned from the first `wallet_path(db_dir)` call (db dir = support dir), so a panic
  before the first wallet call leaves no note. The report adds a "Rust panics" section. The site says "no
  telemetry" (home FAQ + Security row, README): keep those claims true. New log lines must never include
  secrets, even though they're scrubbed.
- **Relay membership check**: `reseed` lets the restorer choose members and threshold and
  the blind relay can't verify them, so `node::check_relay_membership` (route
  `POST /v1/mailbox/info`, body `MembersRead`, response `MailboxInfo`; no `RELAY_API` bump,
  404 = older relay -> members route only) compares them with the replayed log in
  `load_state` and `reseed_relay` (cached 60 s per relay URL + mailbox; tolerates a seat move
  the relay hasn't applied). Mismatch = `NodeError::RelayMembership` -> `ZafeErrorKind::
  RelayMembership` -> sync failure `relayMembership`. Recovery is a fresh relay.
- **Leaving a fork**: `node::follow_relay` (bridge `follow_relay`, sync sheet "Follow the
  relay instead") is the only caller of `LogCache::remove` besides vault removal: needs a
  real fork, verifies the relay's log from entry 0 without the anchor, then replaces the
  copy and reports the local-only entries dropped. Always behind a dialog + unlock.
- **Unapproved-spend grace** (`spend_watch`) only covers Approved/Broadcast proposals and
  the txid their PCZT fixes; cancelled, rejected and open ones get none.
- **Relay is blind**: it only sees public keys, ciphertext, metadata. Clients drop envelopes
  for another mailbox, from non-members, badly signed, or with non-increasing seq.
- **Relay rollback and loss** (spec §6.3; `log_cache`, `node::{load_log, reseed_relay}`):
  every device keeps the verified entries of each vault's log (`<dir>/<vault id>.log`,
  grows only, atomic writes; never shrinks even when two isolates race). `load_log`
  rebuilds the chain from it (re-verifying), asks the relay for the copy's **last entry**
  and refuses a relay that has no such entry (`NodeError::RelayRolledBack`), another
  entry there (`RelayForked`, also any `ChainError::Fork`) or no mailbox (404 on
  `/v1/log/read`: `RelayLostVault`); typed through the bridge (`ZafeErrorKind::
  RelayRolledBack/RelayForked/RelayLostVault`, Dart `SyncFailureKind`, sync sheet
  "Restore vault on the relay"). No copy (first load, new install, restored backup) = the
  first log is trusted once. The directory is set once per process/isolate:
  `log_cache::configure` (CLI: `<home>/log`; app: `init_log_cache(ZafePaths.logDir)` in
  `main()` and in the background isolate's `_ensureRust`); `RelayClient::new` picks it up,
  tests use `RelayClient::with_log_cache`. The bridge's `identity()` fails closed when it
  isn't configured, so bridge tests call `init_log_cache` first. Restore
  (`reseed_relay`, bridge `restore_relay`, CLI `zafe restore-relay`): `POST
  /v1/mailbox/reseed` (`Reseed`, timestamped, no schema change: the mailbox stays
  unsealed with a zero join-token hash until the last batch sets `finish`, and only its
  maker can continue it) recreates a relay-lost vault from the copy; a relay that holds a
  prefix of the same log is caught up with ordinary appends; a relay whose entry at the
  shared tip differs is refused (`RelayForked`). The relay doesn't require entry authors
  to be current members (seats move; clients verify). Do **not** build anything that
  changes the replay of an entry already in a log without an event-version gate.
- **Unapproved-spend alert** (`spend_watch::unapproved_spends`, `VaultWallet::
  vault_spends`, bridge `unapproved_spends`, Home card + notification): a vault spend
  the log doesn't account for (no proposal logged as sent with that txid, and not a
  recent spend of a logged proposal's notes within `GRACE_BLOCKS`). Reads the wallet's
  `ironwood_received_note_spends`; the alert never blocks anything.
- **Test coverage notes (2026-10-07)**: `regtest_e2e` checks `VaultWallet::vault_spends`
  against a real mined spend; `bridge_e2e` asserts no unapproved spend once the log has the
  txid, and keeps the interactive path by letting only member A publish a pool (no fully
  covered group, `!one_tap`). `m0-e2e.sh` is one-tap now (CLI keygen publishes pools):
  `approve` x2 then `send`. Local disk: `target/` grows past 60 GB; delete
  `target/debug/incremental` and `examples` when a link step reports "No space left".

## Dependency gotchas

- **State dir names** live in `zafe_core::state_dir` (`NONCES`, `POOL`, `LEADER`, `REPAIR`,
  `SENT`, `USED_COMMITMENTS`, `REQUEST_EXT`, `OWN_SHARES_EXT`); the CLI's own home files
  are consts at the top of `zafe-cli/src/main.rs`. Never spell a path name inline.

- Exact pins live in `Cargo.toml` (spec §4.4): `reddsa =0.5.2` (`frost` feature; 0.6 removed
  FROST), `frost-core`/`frost-rerandomized` 3.0.0, `orchard =0.15.5` (no `unstable-frost`
  needed), `pczt =0.9.3`, `zcash_client_backend =0.24.0`, `zcash_client_sqlite =0.22.0`.
- `zcash_client_backend/pczt` turns on `transparent-inputs`; `zcash_client_sqlite` must enable
  `transparent-inputs` **and** `serde` or it fails to compile.
- `zcash_keys` feature `unstable-frost` gives `UnifiedFullViewingKey::from_orchard_fvk`.
- Messaging crypto is the **previous generation** (`ed25519-dalek` 2, `hpke` 0.12,
  `chacha20poly1305` 0.10): the latest (dalek 3 / hpke 0.14) needs stable `sha2` 0.11, which
  conflicts with `bip32`'s `sha2 =0.11.0-pre.4` pin via zcash_client_backend.
- `rusqlite` must use `bundled` at the workspace level (the relay links it on its own).
  Only `zafe-core` adds `bundled-sqlcipher-vendored-openssl` (zcash_client_sqlite 0.22 pins
  rusqlite 0.37 / libsqlite3-sys 0.35, same as ours). libsqlite3-sys picks SQLCipher's
  crypto provider at build time: OpenSSL (vendored = `openssl-src` builds a static
  libcrypto, needs `perl` + `make` on the build host, cross-compiles with the NDK clang
  cargokit sets up), system OpenSSL via `OPENSSL_DIR`, or CommonCrypto on Apple when
  neither is set. There is no pure-Rust provider in rusqlite. Vendored was chosen so
  Android (no public system libcrypto) and every other target build the same way; iOS
  could drop to CommonCrypto later to save size. Feature unification means a workspace
  build links SQLCipher into the relay too; without a key SQLCipher behaves as plain
  SQLite, and `cargo build -p zafe-relay` alone stays plain `bundled`. The relay DB stays
  unencrypted on purpose: it holds only public keys, ciphertext and metadata (use disk
  encryption on the host).
- `[profile.dev.package."*"] opt-level = 3`: Halo 2 is unusable unoptimized.
- `zcash_primitives` `non-standard-fees` is a **dev-dependency only** (tests model an
  overpaying proposer).
- `cargo build -p a -p b --examples` builds only examples — build bins separately.
- **TLS (clients)**: rustls + **ring** + **webpki-roots** (bundled Mozilla roots)
  everywhere: reqwest `rustls-tls` (relay client when direct, relay's FCM client; over
  Tor the relay client uses `zcash_client_backend`'s hyper + tokio-rustls) and tonic
  `tls-ring` + `tls-webpki-roots` (+ `zcash_client_backend/lightwalletd-tonic-tls-webpki-roots`).
  Why: no OpenSSL to cross-compile, ring builds with the NDK (aws-lc-rs needs cmake/NASM),
  and bundled roots behave the same on Android/iOS without platform-verifier plumbing
  (trade-off: roots update with the crate, not the OS; no user-installed CAs). Keep only
  one rustls provider in the tree (ring): with both, rustls can't pick a default.
  Gotcha: `tonic::Channel::from_shared("https://…")` does **not** turn TLS on
  ("Connecting to HTTPS without TLS enabled"); `wallet::connect` sets
  `ClientTlsConfig::new().with_webpki_roots()` for https. `RelayClient::with_extra_root`
  adds a trust anchor (private CA / tests) and keeps verification on. Transport errors
  now carry their cause chain (e.g. `UnknownIssuer`).
- **Tor ("Use Tor", Settings > Privacy; default off, per device)**: arti embedded through
  `zcash_client_backend`'s `tor` feature (arti-client 0.35, no tor binary; +252 crates,
  ring only, zstd/xz C code builds with the NDK; no pinned crate moved). Size: arm64
  `librust_lib_zafe.so` stripped 24.7 → 34.7 MB (+10 MB; gzip 11.8 → 15.8 MB, i.e.
  about +4 MB to download). Policy in
  `zafe_core::tor`, process-wide, **fail-closed**: `tor::request()` flips the route
  before any bootstrap (and cancels a `CancellationToken` every direct connection holds);
  from then on `tor::route()` returns the Tor client or `Blocked::{Connecting, Failed}`
  (→ `NetFailure::TorConnecting/TorFailed` → `ZafeErrorKind::TorConnecting/TorFailed` →
  `SyncFailureKind::torConnecting/torFailed`), never direct. A request waits up to
  `ROUTE_WAIT` (20 s) for a bootstrap in progress; a failed one answers at once.
  `tor::enable(dir, budget)` is bounded (arti retries forever on a blocked network),
  idempotent (instant when Ready), serialized, abandons when Tor is turned off
  mid-bootstrap, and on failure leaves status `Failed` with the route still Tor. Choke
  points: `wallet::connect` (every lightwalletd use: sync, send, mempool watch,
  `check_server`, `chain_tip`, endpoint checks) and `RelayClient::exchange` (every relay
  request incl. `/v1/wait`, push registration, `/health`). Direct paths: lightwalletd
  uses tonic `connect_with_connector(tor::DirectConnector)` (our TCP, tonic adds TLS;
  I/O wrapped in `DirectIo`, which errors once the token is cancelled, and the
  connector refuses tonic's reconnects); the relay keeps **reqwest** for direct and wraps
  each request in `DirectLease::guard` (dropping the future drops the connection; pooled
  idle connections are never used while Tor is on). Through Tor: lightwalletd via
  `Client::connect_to_lightwalletd` (90 s cap), relay via `Client::http_get/http_post`
  (why not a pooled hyper client over arti: `zcash_client_backend::tor::Client` doesn't
  expose its arti client, and a second arti instance would duplicate it; cost: a new Tor
  stream + TLS handshake per relay request, ~1 s; timeouts get +30 s). Relay and
  lightwalletd use separate isolated circuits. `RelayClient::with_extra_root` is not
  honoured over Tor (bundled roots only). arti refuses local addresses, so regtest
  (127.0.0.1 / 10.0.2.2) can't work with Tor on: it fails closed, as it should. App:
  pref `zafe_use_tor`; `main()` calls `torRequest()` right after `RustLib.init()`
  (before anything can connect), `torLifecycleProvider` (kept alive by `ZafeApp`)
  bootstraps without blocking the UI (180 s) and sets dormant on hide/pause (a request
  wakes arti by itself), retrying a failed Tor on resume. Background checks
  (`vault_watch.dart`) read the pref after `prefs.reload()`, then `ensureTorForBackground`
  (request, then enable within 60 s) and skip the run if Tor isn't connected. State dir:
  `<appSupport>/tor` (`ZafePaths.torDir`; Android backup/transfer already excluded; on
  mobile fs-mistrust is relaxed with `dangerously_trust_everyone`, the sandbox being the
  boundary). FCM push stays direct (Google, content-free). Tests: `tor::tests` (pure
  route decision, `DirectIo` cut, guard), `tests/tor_policy.rs` (own process: nothing
  reaches a server once Tor is requested, waiters wake on failure, a direct long poll
  and a lightwalletd call are cut; ignored live test: cold bootstrap ~14 s, testnet tip
  through Tor ~3.5 s, an HTTPS GET ~1 s), Dart `test/tor_setting_test.dart`; sheet
  preview `flutter test tool/screens/tor_render_test.dart`. Not done: iOS backup
  exclusion of the Tor dir, onion endpoints, per-vault circuit isolation.
- **Relay limits** (`zafe_relay::limits`): off in `Relay::new()` (tests), on in the
  `zafe-relay` binary (`Limits::hosted()`: 300/min per key, 1200/min per IP). Charge a
  key only after its signature verifies (`verified(relay, body)`, and after
  `envelope.verify` / `verify_signature` in the envelope and log handlers), or anyone can
  drain a member's bucket. The IP middleware needs `into_make_service_with_connect_info`
  (without ConnectInfo and no proxy header it skips). New flows that poll the relay must
  stay under the hosted rate: `node_keygen` and `bridge_e2e` run with it.
- **Relay quotas** (`zafe_relay::quota`): same pattern (`Relay::with_quotas`, off in
  `Relay::new()`, `Quotas::hosted()` in the binary: 10k undelivered envelopes per
  recipient, 256 MiB deliveries + 512 MiB log per mailbox). Byte totals are running
  counters on `mailboxes`: **every write or delete of `deliveries` / `log_entries` must
  update them in the same transaction** (`post_envelope`, `log_append`, `prune`). Over
  quota → `RelayError::QuotaExceeded` → HTTP 507 → `RelayClientError::StorageFull` →
  `ZafeErrorKind::RelayStorageFull`. Gotcha: no `--` comments inside `SCHEMA`'s CREATE
  TABLE text: SQLite stores it and `ALTER TABLE ... DROP COLUMN` then fails to re-parse.
- **Live activity (long polls)**: `POST /v1/wait` (`WaitRequest`/`WaitResponse` in
  `zafe_proto::relay`, `zafe_relay::wait`, `RelayClient::wait_for_activity`) answers when
  the mailbox log is longer than the client's `log_len` or a delivery for the signer is
  past `inbox_after`, else after `max_wait_secs` (capped at `MAX_WAIT_SECS` = 25, below
  proxy idle timeouts). A per-mailbox `tokio::sync::watch` counter is bumped by
  `log_append`/`post_envelope` **after commit and after dropping the DB mutex**; waiters
  subscribe before reading so nothing slips between read and wait, and never hold the
  mutex while waiting. Signed + fresh like other reads, charged to the key's rate limit
  after the signature verifies; concurrent waits capped per key (2) and in total (4096)
  → 429, always on (`Relay::with_wait_caps`); `max_wait_secs = 0` is a plain read and
  takes no slot. Unknown mailbox is **403 here, not 404**, so a 404 unambiguously means
  "relay without the endpoint". Any new write path that should wake apps must call
  `relay.waiters.signal(&mailbox)`. Bridge `api/watch.rs`: `watch_vault(..., watch_id,
  sink)` spawns a loop on the bridge runtime (holds no FRB worker) and returns; events
  `Connected`/`Activity`/`Unsupported`/`Failed{retry_in_secs}` (failures are events;
  backoff 2^n s up to 60, `Retry-After` on 429, stops on version errors). Watches are
  ordered by Dart-chosen increasing ids (`stop_vault_watch(id)` stops that id and older)
  because Dart's sync stop can reach Rust before the async start. Dart:
  `services/live_vault_watch.dart` (`LiveActivityPolicy` + `LiveVaultWatch`, unit-tested)
  run by Home in the foreground; events call `ProposalsNotifier.refreshSoon()` (queues one
  more refresh if one is running); while live the Home poll relaxes from 15 s to 60 s but
  keeps running. Background stays on FCM + WorkManager.
- **Inbox acknowledgement** (`POST /v1/inbox/ack`, `InboxAck { cursors }`, at most
  `MAX_ACK_CURSORS` = 256 per request): deletes the signer's own deliveries by cursor
  (never by range: one inbox mixes a member's signing requests with the shares it
  collects as leader) and lowers `delivery_bytes` in the same transaction. Clients read
  inboxes from cursor 0 every time, so the relay can't infer "picked up" from a read.
  `node::respond` (every poll and background check; the bridge no longer skips it when
  the member holds no nonces, and takes the caller's synced `tip_height`) acknowledges keygen messages (the
  vault exists), signing requests answered or unanswerable (no nonces), undecodable
  envelopes, and shares for closed/expired proposals; a request that failed for a
  passing reason stays. Best effort: a relay without the route (404) or an error
  leaves them to the 30-day retention. No `RELAY_API` bump (new route only).
- **Relay deploy**: `GET /health`; `PORT` → `0.0.0.0:$PORT` unless `ZAFE_RELAY_LISTEN`;
  FCM key from `ZAFE_FCM_SERVICE_ACCOUNT` (file) or `ZAFE_FCM_SERVICE_ACCOUNT_JSON`
  (inline, for Fly secrets). The image's entrypoint chowns `/data` then drops to uid
  10001 with `setpriv` (Fly volumes mount root-owned). The Docker context must contain
  **every workspace member** (`app/rust` too) or `cargo build --locked` fails;
  `.dockerignore` whitelists them. One machine per SQLite file, never scale out.
- **Testnet is a separate Android app**: `ZAFE_NETWORK=test` builds get
  `applicationId xyz.zafe.zafe.testnet` and the label "Zafe Testnet" (`build.gradle.kts`
  reads the dart-define; the manifest label is `${zafeAppLabel}`), so it installs next to
  the mainnet app and can't update it. Mainnet and regtest/dev builds stay
  `xyz.zafe.zafe` (the harness and `adb` commands use that). The site's
  `assetlinks.json` lists both packages. `google-services.json` must contain a client
  for whichever package you build, or the Google Services Gradle plugin fails.
- **Zcash server list** (2026-10-06, Vizor-style): `core/config/lightwalletd_presets.dart`
  holds the public servers per network (first = the build default; mainnet: Zec Rocks
  global/na/eu/ap/sa, Stardust us/eu, Zcash Explorer; testnet: only testnet.zec.rocks
  answered). Settings > Zcash server lists them with a latency probe each
  (`probeLightwalletd`, a `checkLightwalletd` call, through Tor when it's on) plus
  "Custom server". Sync fails over (`serverFailoverProvider`, from `VaultNotifier.sync`)
  to the next listed server on unreachable/timeout/TLS/server-behind, at most once per
  5 min, never away from a custom URL, and the app toasts the move. Background checks
  don't fail over. Re-probe the list (`grpcurl ... GetLightdInfo`) before adding hosts:
  Stardust eu2/jp and two community servers were dead or had broken TLS.
- **Hosted relay (testnet)**: `testnet.relay.zafe.cash` on an OVH VPS (57.129.172.198,
  Ubuntu 26.04), compose in `/opt/zafe-relay`. Never deploy by hand (scp/systemctl): push
  to `main` or run the Relay deploy workflow; it deploys an image **by digest**, and
  `deploy.sh` backs up, switches, health-checks through Caddy and rolls back. The DNS
  record must stay **DNS only** in Cloudflare (two-level name: no universal cert; a proxy
  would see client IPs). Compose gotchas: bind-mount the Caddy *directory* (CI replaces
  files, a single-file mount keeps the old inode); relay containers run as uid 10001 on a
  named volume (Docker copies the image's `/data` ownership into a new named volume, so
  the entrypoint's chown isn't needed and `cap_drop: ALL` works); Docker-published ports
  bypass ufw, so publish nothing but Caddy's.
- **Hosted relay (mainnet, prepared 2026-10-07, not deployed)**: `relay.zafe.cash`, same
  VPS, compose profile `mainnet` (`relay-mainnet`, `backup-mainnet`, `litestream-mainnet`
  to S3-compatible storage, `restore-mainnet` for an empty volume), site block
  `vps/mainnet/relay.caddy` copied in by the mainnet workflow. `deploy.sh <testnet|mainnet>
  <image@digest> <domain>` edits only its network's lines of `.env` and writes
  `deployed/<network>.json`. Only `.github/workflows/relay-mainnet.yml` deploys it
  (manual, environment `relay-mainnet` with required reviewers): it promotes the digest
  testnet runs (record + `min_soak_hours`, build provenance verified); never build or
  deploy mainnet from a push. A compose variable without a default breaks the file even
  when its profile is off: mainnet variables have defaults. Capped beta: relay
  `ZAFE_RELAY_MAX_VAULTS` (507 + `zafe-quota: capacity` → `RelayClientError::AtCapacity`
  → `ZafeErrorKind::RelayAtCapacity`) and the app's `core/config/beta.dart` (beta label
  and per-vault cap on mainnet builds, `ZAFE_BETA`, `ZAFE_BETA_CAP_ZAT`).
  `kMainnetRelayUrl` = `https://relay.zafe.cash`. Steps only the user can do:
  `infra/relay/README.md` "Mainnet"; audit scope: `docs/audit-scope.md`.
- **Public testnet lightwalletd**: `https://testnet.zec.rocks:443` (Ironwood-aware;
  Ironwood live on testnet since block 4,134,000). Mainnet: `https://zec.rocks:443`.
- **A `CARGO_TARGET_DIR` shared between worktrees races** when their workspace crates
  differ: path crates hash relative to the workspace root, so every worktree writes the
  same `libzafe_proto-<hash>.rlib` and a concurrent build can hand yours another
  worktree's version ("could not find `version` in `zafe_proto`"). Use a private target
  dir for verification when other agents build at the same time. Building `zafe-cli` or
  `zafe-relay` into the main checkout's `target/` also replaces the binaries
  `scripts/app-harness.sh` runs.
- `propose_transfer` / `create_pczt_from_proposal` need explicit error type params
  (commitment_tree::Error, GreedyInputSelectorError, zip317::FeeError).

## Illustrations

- **Brand decisions and research**: `docs/brand.md` (2026-10-01: Verdigris palette,
  Patina icons, Seam app icon, light vault card in light mode, gold = money). Redo or
  extend them with the project skills `.claude/skills/brand-palette` (palette + icon
  style; `oklch.js derive <hue>` prints tokens and contrast) and
  `.claude/skills/logo-design` (marks; `marks.js proof out.html`). Implemented
  2026-10-01; `app/lib/src/core/theme/primitives.dart` is generated by
  `node scripts/brand/primitives.js` (edit the generator, not the file).
- App art (onboarding and more) is original SVG generated by Python: guide and recipe in
  `docs/illustrations.md`, code in `scripts/illustrations/` (`zafe_art.py` palette/toolkit,
  `scenes.py` scenes, `scene_template.py`). Never derive from Vizor's art.
- Preview (regenerates, renders through flutter_svg to `app/build/illustration_preview/`):
  `scripts/illustrations/preview.sh [scene]`. In-app screenshots (wipes app data):
  `scripts/illustrations/device-shots.sh`.
- flutter_svg ignores/breaks `<pattern>`, `<polyline>`, `<line>`, filters: the toolkit bakes
  texture fills into geometry; always check renders with `preview.sh`, not a browser.
- The art palette in `zafe_art.py` mirrors `core/theme` ("Verdigris": tinted neutrals,
  `brand` keys = members/brand, `#00736C` family in light mode; gold = keys/funds); `sky1`
  must equal `background.window` (#080B0B / #F1F5F5), and `app/tool/illustrations/
  preview_test.dart` hardcodes the window colours too: update all three together (plus
  `app/tool/brand/render_test.dart`, `emblem_render_test.dart` and the launch colours).
- Art is also used outside onboarding: backup/export/restore banners, the sending screen's
  full-page background (`IllustrationBackground`), the Activity empty state.
- **App icon and splash** are generated the same way: `scripts/brand/brand.py` ("Seam":
  deep verdigris #004A46 tile, a block split by a Z-shaped channel into a pale #D9F5F2
  upper piece and a #51DDD2 lower piece, geometry = `marks.js` `seam()`; flat colours
  only, so iOS Tinted/Clear and Android themed icons repaint it cleanly; monochrome and
  notification versions draw both pieces in one colour) writes `app/tool/brand/svg/`, and `scripts/brand/icons.sh` renders them with
  flutter_svg (`app/tool/brand/render_test.dart`) into
  `mipmap-*/ic_launcher{,_foreground,_monochrome}.png` (the adaptive background is the
  colour `@color/zafe_icon_tile`, no bitmap), `drawable{,-night}-*/splash_{icon,mark}.png`
  (the Seam tile, 144 dp, same in both themes), iOS `AppIcon.appiconset` (full-bleed,
  alpha stripped with ImageMagick) and `LaunchImage.imageset` (light + dark). The mark's
  100-unit box maps to the adaptive icon's visible 72 dp; the 56-unit block's corners
  sit 28.5 dp from the centre, inside the 66 dp safe circle. Preview (small sizes down to 24 px, themed icon, both splashes):
  `app/build/brand_preview/brand_sheet.png`. The splash colour is `@color/zafe_window`
  (`values{,-night}/colors.xml`, also the NormalTheme window background) and the iOS
  `LaunchBackground` colour set (+ the storyboard fallback); keep them equal to
  `background.window` (#080B0B / #F1F5F5). Android 12+ splash
  attributes sit in the base `styles.xml` with `tools:targetApi="31"` (no `values-v31`).
  The splash follows the app's theme setting, not just the OS: `AppThemeHost` sends it
  over `xyz.zafe/window_appearance` (`setBrightness`), and on Android 12+ `MainActivity`
  calls `UiModeManager.setApplicationNightMode`, which the OS persists for the next
  launch's splash. Android < 12 and iOS (no handler yet) still follow the OS theme.
  Check resources without a Gradle build: `aapt2 compile --dir res` + `aapt2 link` against
  `platforms/android-36/android.jar`.
- **Screen previews without a device**: `flutter test tool/screens/home_render_test.dart`
  (vault card, Home buttons, notice card, activity rows, tab bar → `home_{dark,light}.png`)
  and `flutter test tool/screens/proposal_render_test.dart`
  (from `app/`) pumps the proposal body, payment card and signer rows with fake data (fonts
  loaded with `FontLoader`, `proposalReviewProvider` overridden) and writes
  `app/build/screen_preview/*_{dark,light}.png`. Widgets that call Rust (`vault.summary`,
  `myKeyHex`) can't be pumped, so keep screen bodies in public widgets that take plain data
  (`features/proposals/proposal_parts.dart`). To capture a sheet, wrap the whole
  `MaterialApp` in the `RepaintBoundary` and apply `AppTheme` through its `builder`
  (`welcome_render_test.dart`). Widget tests use the Ahem font, which is wider than DM
  Sans: a 390 pt view overflows button labels, so use a wider view there.
- In a fresh worktree `flutter analyze` reports errors in `rust_builder/cargokit/build_tool`
  until `dart pub get` is run in that directory.

## Regtest (infra/regtest)

- **Regtest is `ths`** (thus-spoke-zakura, pinned `THS_VERSION` in `up.sh`, currently 0.3.0:
  Zakura 1.6.0 + its lightwalletd, NU5..NU6.3 at height 1), so our tests also exercise ths
  and catch its bugs. Keep the pin on the latest ths release. Install:
  `curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/zcashlabs/thus-spoke-zakura/main/install.sh | THS_VERSION=0.3.0 sh`
  (to `~/.local/bin`; `THS=/path/to/ths` overrides). `up.sh` refuses another version.
- `up.sh` runs `ths --name $ZAFE_REGTEST_NAME start --no-open --port-offset
  $ZAFE_REGTEST_PORT_OFFSET` under `setsid nohup` (it stays in the foreground and deletes
  the environment when interrupted), pid + log in `~/.cache/zafe-regtest/`, and waits for
  "is ready" (~10 s). Ports: RPC 18232, lightwalletd 9067, dashboard 32805, each + offset
  (multiple of 10, at most 32730). Offsets in use: harness 0, `regtest_e2e` 30000,
  `bridge_e2e` 30100, `m0-e2e.sh` 30200. `down.sh` sends SIGINT, then `ths stop`.
  Gotcha (0.3.0): `ths stop` deletes the environment but leaves the foreground launcher
  running; only SIGINT ends it (fix: thus-spoke-zakura PR #146, issue #145).
- **Funding**: ths mines to its own wallet, so vaults are funded from the faucet:
  `fund.sh <ua> [notes]` = `notes` x 5 ZEC (the faucet's maximum) to the Ironwood receiver,
  then 12 blocks so they pass the default confirmation policy. One note per proposal that
  holds notes at the same time (reservation), so tests ask for several. Faucet receipts are
  ordinary payments: nothing tests coinbase receipts (`is_coinbase`) any more.
  Mine with `ths --name <name> mine N` (fast; proof of work on costs ~3 s per 120 blocks).
- Every start is a **fresh chain** (ths keeps none), so `app-harness.sh resume` refunds
  the vault and deletes the CLI members' wallets.
- Background (no longer our config): NU6.1's activation block needs a ZIP 271 lockbox
  disbursement; ths uses the zero-value marker `t26YoyZ1iPgiMEWL4zGUm74eVWfhyDMXzY2`.
  Coinbase to a unified address lands in Ironwood.

## Mobile findings (spec V7/V8)

- Phone (Snapdragon SM8735, Android 16): proving key 3.4 s / proof 4.6 s single-thread,
  2.1 s / 1.7 s on 8 threads, ~113 MB. Round 2 (verify + sign) 12 ms, 6.8 MB → fits an iOS
  Notification Service Extension (24 MB, ~30 s). Only the leader proves.
- WSL2 has no USB: use adb **wireless debugging**; `adb pair IP:PORT CODE` must have the code
  on the same line (non-interactive shell). NDK r29 at `~/android/android-ndk-r29`,
  `cargo ndk -t arm64-v8a`.

## M1 app: built on Vizor (chainapsis/vizor-wallet, Apache-2.0), with Zafe's own look

Vizor is the reference for **architecture and components**, but the user asked (2026-09-30)
that Zafe **not look like a copy**. Zafe's visual identity, keep it when resyncing:
- **Palette ("Verdigris", chosen 2026-10-01, `docs/brand.md`)**: neutrals tinted ~1%
  toward hue 188 (`Primitives`), **verdigris** brand (`BrandPrimitives`: #51DDD2 dark,
  #00736C light), primary buttons #51DDD2 with a #001B19 label (dark) and #00736C with a
  white label (light). **Zcash gold = value**: received money, "+x TAZ", the ticker,
  success (`GoldPrimitives`, #F3BA3C / #835A00; never green: teal and green are too
  close). Warnings orange (`OrangePrimitives`, #FFAE81 / #A55115), errors rose
  (`RosePrimitives`, #E96CAD / #A82571). Window #080B0B / #F1F5F5. The vault and payment
  cards use `colors.vaultCard` (`AppVaultCardColors`): ink #091312 in dark mode, a light
  #E0F3F1 card with a line border in light mode (user's decision), dial drawn in the
  accent (70% in light). Earlier: "Signal" lime + charcoal (2026-09-30), jade + slate.
  **Token names follow the three colour jobs** (renamed 2026-10-01): value =
  `text.value`, `icon.value`, `background.valueAlpha` (were `positiveStrong`/`success`,
  `utilitySuccessAlpha`); errors = `*.destructive*` (the `utility` prefix is gone:
  `background.destructiveSubtle/Strong/Alpha/AlphaSubtle`, `border.destructive/
  destructiveSubtle`); `background.darkCard` / `text.darkCard` (were `homeCard`: the
  dark QR card, not the vault card); `sync.{text,textError,glow}` only. Unused gold
  leftovers (`utilitySuccessSubtle/Strong`, `border.utilitySuccess/utilityPositiveStrong`,
  `text.success`, `sync.textSyncing/lightSuccess/lightError`) were deleted.
  Material widgets use `core/theme/material_theme.dart` (was `legacy_material_theme`,
  Vizor's gray/green scheme): its `ColorScheme` and text selection come from `AppColors`,
  body font DM Sans.
- **Type**: Space Grotesk (display, amounts, titles), DM Sans (body), JetBrains Mono (codes).
- **Shapes**: rounded-rectangle buttons (`zafeButtonRadius`: 14 / 10 / 8), card radius 20,
  rounded-square avatars; activity icon tiles are circles (Patina).
- **Icons ("Patina two-tone", docs/brand.md §4)**: **every** `AppIcons` name renders
  Phosphor (MIT) SVGs from `assets/icons/patina/<name>{,_active}.svg` (generated by
  `scripts/brand/patina_icons.py`, which also keeps the licence in
  `assets/icons/licenses/`): black = outline in the icon colour (thickened to 1.7 px),
  magenta = closed shapes at 38% (`PatinaColorMapper`): the brand accent, or the icon's
  own colour when that is a semantic one (warning, error, value; `patina:` overrides);
  `AppIcon(active: true)` = the solid fill (current tab, a payment that needs your
  vote). Line icons (x, check, carets, plus, minus, arrows) use Phosphor's regular
  weight, no fill (`LINE_ONLY`). Exceptions: `AppIcons.marks` (the Zcash currency
  glyph, `assets/icons/zcash_currency.svg`, tinted) and the code-drawn `loader`. Add an
  icon to the script's `MAP` and to `AppIcons` together; unused icons are deleted, not
  kept. Contact sheet: `flutter test tool/screens/icons_render_test.dart` →
  `build/screen_preview/icons_{dark,light}.png`.
- **Signature pieces**: home "vault card" (`BalanceCard`: safe-dial rings + accent glow,
  signer-dot threshold strip); the same card for payments (`PaymentCard` in `core/widgets/mobile/zafe_detail.dart`,
  proposal page and send review); signers as `SignerRow`s with key-derived rounded-square
  `SignerTile`s (hue + mirrored 5x5 pattern from the key; "You" outlined in the brand accent) and
  `ApprovalDots` for votes; pushed pages use a boxed back button with a left-aligned 24 px
  title. Icon: Seam on a deep verdigris tile (`scripts/brand/`).
- **No Vizor references in code comments** (the user asked); attribution stays in `NOTICE`
  and the licenses page.
- `ZAFE_FORM_FACTOR` defaults to `mobile` (the phone app previously used desktop tokens
  because no build passed the flag).

What still follows Vizor:
Flutter (pinned **3.41.6** via fvm) + `flutter_rust_bridge` **2.11.1** + Rust core; Riverpod
+ go_router; `flutter_secure_storage`; design tokens with Desktop/Mobile sets selected at
build time by a `--dart-define` form-factor flag; sentence-case copy; Rust API surface
limited to primitives/flat structs (complex types stay behind it); bootstrap snapshot before
the first frame; broadcast-before-store for PCZT sends. Zafe's vault account is exactly
Vizor's Keystone account shape (UFVK-only, external signer), with FROST instead of a device.
Keep attribution/NOTICE for anything copied from Vizor.

Upstream is **github.com/chainapsis/vizor-wallet** (not the stale `valargroup` mirror the
first study used). Detailed reference (tokens, components, screens, bridge setup):
`docs/vizor-reference.md` (§11: resync notes). `lib/src/core` was resynced to upstream
`4bff2e7`; check upstream for newer work and resync file by file, re-applying Zafe edits
(AppButton semantics, `xyz.zafe/*` channels, no Vizor background PNG in the progress screen).
Upstream features to borrow later: settings screens, address book. (Tor is done: see
"Tor" under Dependency gotchas.)
Learned while studying it:
- **Copy** architecture, tokens, component specs, screen structures, and the Keystone
  signing UX (it starts proving in the background while the external signer works — do the
  same while FROST round 2 runs). Keep Apache-2.0 attribution + a modification notice.
- **Do not copy** Vizor's knight/castle illustrations and profile pictures (brand identity, no
  documented origin), the Vizor name/wordmark, `com.keplr.vizor` bundle IDs or
  `com.zcash.wallet/*` channel names.
- **Fonts** are OFL 1.1 static instances from Google Fonts (`fonts.googleapis.com/css2`
  without a browser UA returns TTF URLs); bundle each family's OFL text in
  `assets/fonts/licenses/` and register it in `main.dart`.
- **Improve on Vizor**: encrypt the wallet DB (SQLCipher, spec §14), typed errors across the
  bridge instead of substring matching.
- FRB: mark cheap calls `#[flutter_rust_bridge::frb(sync)]` (otherwise Dart gets a Future).
  `flutter_rust_bridge_codegen generate` works without `cargo-expand` (it only warns).
  The bridge crate `app/rust` (`rust_lib_zafe`) is a workspace member so it shares pins.
- App layout: `lib/src/core/` is copied Vizor code (keep in sync with NOTICE); Zafe code is
  `lib/src/{providers,features}`, `lib/src/app.dart` (GoRouter + redirect on vault state),
  `lib/main.dart` (RustLib.init → `VaultBootstrap.load()` → ProviderScope override).
  Secrets (identity, invite, key material) live in `flutter_secure_storage` via
  `core/storage/zafe_secure_store.dart`. **Its calls can get no reply** on Android when
  a background engine (WorkManager/FCM check) uses the storage at the same time: every
  call goes through `_PatientStorage` (10 s timeout, one retry); never call the plugin
  directly. The **network** is compile-time (`ZAFE_NETWORK`,
  default regtest, `core/config/network_config.dart`). Relay/lightwalletd **defaults**
  come from the build: presets per network (`regtest` local http; `test`/`testnet`:
  zec.rocks TLS lightwalletd + a placeholder relay `https://relay.zafe.invalid` until one
  is deployed, shown as "Not configured" in Settings), overridden by the dart-defines
  `ZAFE_RELAY_URL` / `ZAFE_LIGHTWALLETD_URL`. Dart const expressions can't read fields of
  const objects, hence the parallel consts. **At runtime** the user can override either
  URL in Settings (`features/settings/endpoint_sheet.dart`): `core/config/endpoints.dart`
  (`ZafeEndpoints`, `checkEndpointUrl`: https required except localhost/127.0.0.1/
  10.0.2.2 on regtest; path/query refused; typed ports kept, since Dart's `Uri` drops a
  default `:443`) stores them in prefs keyed per network (`zafe_relay_url_<net>`,
  `zafe_lightwalletd_url_<net>`; saving the default removes the key). The sheet tests
  the URL first (bridge `api/endpoints.rs`: `check_relay` = `GET /health` must answer
  `ok`; `check_lightwalletd` = `GetLightdInfo`, network must match). **Never use
  `kZafeRelayUrl`/`kZafeLightwalletdUrl` directly**: UI code reads
  `ref.read(endpointsProvider)` (from the bootstrap), background checks use
  `ZafeEndpoints.fromPrefs(prefs)` after `prefs.reload()` (prefs cache per isolate). A
  relay change re-registers the push token (`reregisterPush`). All members of a vault
  must use the same relay.
- **Site download link**: `site.yml` sets `ZAFE_DOWNLOAD_URL` to `https://github.com/zafe-cash/zafe/releases` (the testnet APKs are pre-releases, so `/releases/latest` 404s).
- **Bridge errors are typed**: API functions return `Result<T, ZafeError>` (`api/error.rs`,
  `kind` + `message`); Dart maps `ZafeErrorKind` to copy in `core/errors/zafe_error_copy.dart`.
  FRB treats a `type Result<T> = ...` alias as **anyhow** — always write
  `Result<T, ZafeError>` in public signatures or the typed error silently disappears.
- FRB `StreamSink<T>` gives Dart a `Stream` (used for send progress). A streaming
  function's returned `Err` never reaches the Dart listener (unhandled exception), and
  `sink.add_error(ZafeError)` arrives as an undecodable `AnyhowException`: report failures
  as a normal event (`SendStage::Failed` + `error: Option<ZafeError>`). The generated Dart
  `ZafeError` has no useful `toString`; log with `describeError`. Keep a plain-callback
  twin marked `#[frb(ignore)]` (`send_with_progress`) so Rust tests can drive it.
- **New in-vault routes must be added to the `inVault` list in `app.dart`'s redirect**,
  or opening them bounces to `/home` with no error (hit with `/scan-recipient`).
- **Vault tabs** (2026-09-30): `/home`, `/activity`, `/signers`, `/settings` are the
  branches of a `StatefulShellRoute.indexedStack` (`features/home/vault_shell.dart`,
  Vizor's floating `AppMobileTabBar`, `NoTransitionPage`); switch with `context.go`,
  never `push` (pushing a tab root stacks a second copy). Every other in-vault route is
  top-level, so `push` covers the tab bar. Tab screens keep ~112 px bottom padding for
  the floating bar and use `MobileTopNav.back` without `onBack`. Home stays mounted
  while another tab shows (indexed stack), so its 15 s poll keeps running.
- **Vault emblem** (`features/vaults/vault_emblem.dart`): Home's avatar and the
  switcher show one of 8 drawn motifs (dial, keyhole, peaks, waves, coins, gem, sun,
  orbit) on a dark tile in one of 6 Verdigris/gold/bronze palettes, both picked by an
  FNV-1a hash of the vault id (keep the palette count and order), so every
  member sees the same picture. Preview: `flutter test tool/screens/emblem_render_test.dart`
  → `app/build/screen_preview/vault_emblems.png`.
- **Cached balance** for the switcher lives in secure storage (`ZafeSecureStore.balance`,
  `zafe_vault_<id>_balance`, removed with the vault), not in `summary.json`; writing the
  summary drops the plain-text balance older builds stored there.
- **Vault name** is the creator's (signed in the descriptor); a member can rename it
  on this device only (`core/storage/vault_name.dart`, `<vaultDir>/vault_name.txt`,
  `vaultNamesProvider`, `activeVaultNameProvider`). Show `activeVaultNameProvider`,
  not `summary.name`; background checks use `VaultName.display`. Not in backups yet.
- **Android app category**: the manifest's `android:appCategory` has no finance value
  (game/audio/video/image/social/news/maps/productivity/accessibility only), so launchers
  that group apps (Nothing) file a sideloaded APK under "Other"; a Play Store listing's
  category is most likely what puts it under Finance (unverified).
- Flows: `/send` (recipient → amount → review → "Propose payment"), `/received/:txid`
  (money received), `/proposal/:id`
  (independent check on this device, votes, approve/reject, "Collect signatures & send"),
  `/proposal/:id/send` (Vizor's transaction progress screen; the send lives in
  `ProposalsNotifier`, so leaving the screen doesn't stop it), `/activity` (all payments).
  Home polls every 15 s (60 s while the live watch is connected, see "Live activity"):
  proposals refresh + answering signing requests, then wallet sync.
  Wallet DB access is serialized by `wallet_lock()` in the bridge.
- **Received payments** (`wallet::VaultWallet::received_payments`, bridge
  `api/received.rs` `list_received`, `providers/received_provider.dart`): `WalletDb`
  doesn't expose its connection, so it opens a second **read-only** rusqlite connection
  (path kept on `VaultWallet`) and queries the `v_received_outputs` /
  `v_received_output_spends` views: non-change outputs of transactions that spend no
  vault note, `tx_index = 0` = coinbase, block time from `blocks`. Must run under
  `wallet_lock()`. The provider reloads whenever the active vault's `Balance` changes
  (every sync bumps the height). Activity rows are `ActivityItem`s
  (`features/proposals/activity_feed.dart`, `mergeActivity`): pending receipts first,
  then by time. The notification snapshot (`seen.json`) now also holds `rx:<txid>` keys
  plus an `rx:*` marker; without the marker (older snapshots) receipts are recorded but
  not announced, so an upgrade doesn't replay history. `recordSeen(id, proposals,
  received:)` merges (a `null` list keeps that kind). Notification payload
  `vaultId:rx:<txid>` opens `/received/<txid>`.
- **Pending receipts: the mempool watch** (`zafe_core::mempool::watch`, bridge
  `api/mempool.rs`, app `providers/mempool_watch_{policy,provider}.dart`). Block sync
  never sees unmined transactions, so while the app is in the foreground the watch reads
  lightwalletd's `GetMempoolStream` (whole mempool, never a txid lookup), parses each tx
  at tip + 1, trial-decrypts it with the vault UFVK **without touching the DB**, and only
  for vault transactions calls `store` on a blocking thread, which takes `wallet_lock()`,
  opens the wallet with its key (`VaultWallet::open`, encrypted path) and runs
  `store_mempool_tx` (`decrypt_and_store_transaction`, mined height `None`). The lock is
  never held while waiting on the stream. lightwalletd closes the stream at every block:
  reconnect after 1 s; real errors back off 1, 2, 4 ... 30 s (reset once a stream
  delivers). Handled txids are remembered (bounded) since each reconnect resends the
  mempool; a failed store is not, so it is retried after the next block. Lifecycle: one
  watch per process. `begin_mempool_watch()` (sync) returns an id and makes older ids
  stale; `watch_mempool(id, ...)` spawns on the bridge runtime, returns at once and runs
  while its id is current (cancel polled every 100 ms, also mid-connect/read/sleep);
  `stop_mempool_watch()` (sync) bumps the id. The app takes the id **before** its awaits,
  so a stop during setup can't be lost. Events: `Connected`, `Stored { txid }` (the app
  reloads `receivedProvider`), `StoreFailed`, `Disconnected { retry_in_secs }`, `Failed`
  (setup error; the stream then closes). `MempoolWatchController` keeps one watch matching
  `mempoolWatchTarget(foreground, activeId, synced = balance != null, lightwalletdUrl)`
  and restarts one that ended by itself after 10 s; `ZafeApp` keeps the provider alive.
  Foreground = `resumed`/`inactive`/unknown. Background checks don't watch (they sync).
  `ReceivedNotifier.refresh` now queues one more read if called while loading.
- `VaultWallet::create` does all network calls **before** creating the DB file; the bridge
  also deletes a DB with no account (`VaultWallet::exists`). Previously the first sync with
  lightwalletd down left an empty DB that failed forever ("expected one account, found 0").
- Vizor's `AppButton` used an onTapUp-only detector (no semantics tap action); Zafe wraps
  it in `MergeSemantics(Semantics(button, enabled, onTap, Focus(...)))`. `Focus` must stay
  **inside** the merge (outside, it adds an unlabeled focusable node and the label reads
  as a separate node); `test/app_button_semantics_test.dart` guards it. Use
  `expand: true` inside `Expanded` rows.
- **Seat moves / share repair** (spec §10.1, §10.4.2; `zafe_core::repair`): a signer who
  lost their phone and has no backup gets their seat moved to a new phone. Membership is
  now **dynamic**: `VaultState.descriptor` is the *current* membership (seat moves applied),
  not the signed original; `node::load_log` checks the `Created` entry against its own
  descriptor and every later entry against the membership as of that entry (so the
  creator can be replaced). Anything membership-related must use the replayed state
  (`members_by_pk(state)`, `frost_id_of(&state.descriptor, ..)`), never
  `material.descriptor` (which the app refreshes from `ProposalList.updated_material`, and
  `load_state` only compares on group key + UFVK). Flow: new phone shows a recovery code +
  8-digit safety code (`RecoveryRequest`), co-signers log `ReplaceApproval` (signature over
  `relay::ReplaceApproval`, reused for the relay), at t the seat moves (votes/shares/groups/
  name re-keyed; interactive approvals voided; pool + backup attestation dropped); the
  relay swaps keys on `/v1/mailbox/replace` with t signatures (t recorded by the creator via
  `/v1/mailbox/threshold`, called by `node::create_vault`; 0 = can't move);
  `repair::help_repairs` (run by the bridge's `list_proposals`, CLI `zafe seat repair`)
  does the first t approvers' RTS rounds (own deltas saved in `<state>/repair/<index>.delta`
  before sending, `.done` after the sigma); `repair::try_recover` (bridge `check_recovery`,
  CLI `zafe recover --wait`) finds the vault via `/v1/mailboxes`, reads the log with a
  helper's log key, requires every helper's sigma with identical vault data, and checks
  the verifying share, group key and UFVK. `load_state` boxes `load_log` (an unboxed future
  overflowed rustc's query depth in `send_with_progress`). App: Welcome → "Lost your
  phone? Recover without a backup" (`/recover`, pending identity in secure storage
  `zafe_recovery_identity`; once done the member must confirm the vault address's last 8
  characters with a co-signer (`VaultCheckBody`), because the new phone can't know which
  vault to expect and a dishonest relay could present one of its own; then
  `addRestoredVault` and `/backup-prompt`); Signers tab →
  "Replace a lost phone" (`/replace-signer`, extra `(oldKey, code)` for a pending move) and
  `SeatMoveCard`s. Tests: `crates/zafe-core/tests/repair.rs` (creator replaced, same share,
  signs with a co-signer), `tests/vault.rs` replay rules, `app/rust/tests/repair_bridge.rs`
  (bridge calls, no Docker). Preview: `flutter test tool/screens/repair_render_test.dart`.
  Stalled repairs: `Replacement { helpers, attempt, done }`; any non-helper member can
  `repair::retry_repair` (bridge `retry_repair`, CLI `zafe seat retry`, Signers tab
  `RepairCard` "Help instead of X") to start attempt + 1 with itself in a helper's place;
  helper state files are `<index>-<attempt>.{delta,done}`; the new phone logs `RepairDone`
  from the bridge's `list_proposals` (CLI `zafe recover`), and `ProposalList.repairs` lists
  unfinished ones. QR: `/scan-recovery` (`ScanRecoveryScreen`) from the Replace screen.
  Notifications: pending moves are announced once (`mv:<old>:<new>` keys + `mv:*` marker in
  `seen.json`; not to the old key or members who approved; tap opens Signers), and
  background checks save `updatedMaterial`. Live-tested on the emulator
  2026-10-01 both ways (app as helper, app as the new phone) with CLI members
  (`zafe recover`, `zafe seat approve|repair`). Tips: `/recover` is a SecureScreen
  (screenshots are black: read it with `agent-device snapshot`); the code is not shown as
  text, so "Copy code", then paste it into Join's field to read it.
- **Backup health** (spec §12.2): `VaultEvent::BackupVerified { epoch, at }` (only for the
  current descriptor epoch; latest per member in `VaultState.backups`), written by
  `node::attest_backup` (once per epoch; bridge `attest_backup`, CLI `zafe backup`).
  `export_vault_backup` decrypts the backup it just made before returning it. The app
  sets the local `summary.json` `backedUp` only once the file was shared (share result
  not `dismissed`; `unavailable` counts) or the text copied, and `ProposalsNotifier`
  attests whenever `backedUp` is set but the log's `ProposalList.backed_up` lacks this
  member (covers exports, restores and devices backed up before attestations; once per
  vault per session). Signers tab: `BackupHealthCard` + `BackupLabel` per row (preview
  in `tool/screens/protection_render_test.dart`).
- **Signer names** (`core/storage/member_names.dart`, `names.json` per vault) travel as
  `Vec<SignerName>` (`api/names.rs`) to `export_vault_backup` / `export_history_csv`
  and back from `import_vault_backup`. Background checks read `names.json` themselves
  (`MemberNames.read`, no providers in that isolate) and pass `names:` to `vaultUpdates`.
- **Shared names**: each member can also name *themselves* in the log
  (`VaultEvent::Name`, `node::set_name`, bridge `set_my_name`; only the author, trimmed,
  ≤ 32 chars, no control chars, empty clears). `ProposalList.shared_names` carries them;
  display uses `MemberNames.merge(shared, local)` (`signerNamesProvider`): this phone's
  label wins, then the shared name, then the short key. Tiles stay key-derived, so a
  member calling themselves "Bob" still shows their own tile and key. "Your name" is the
  sheet behind your own row on the Signers tab; the CLI has `zafe name`.
- **CSV recipients import** (`features/send/recipients_csv.dart`): pure parser with the
  bridge validators injected (`checkAddress` for `kZafeNetwork`, `parseZec`,
  `memoLength`), so it's unit-tested without Rust. All-or-nothing.
- **Viewing key** (`/viewing-key`, in `inVault`, SecureScreen): bridge
  `vault_viewing_key(material)` derives the UFVK and refuses if it differs from the
  descriptor's `ufvk` (`Verification`).
- Pushed-page titles are 24 px and left-aligned (room for ~20 characters); keep them short.
- **Privacy mode is app-wide** (`privacyModeProvider`, persisted in prefs, read in the
  bootstrap): every amount goes through `amountWithTicker(text, hide:)`. Vizor's
  `hideAmountIfPrivacyMode` only appends the unit to the *mask*, so passing a bare amount
  drops the ticker when visible. Payment rows use a 3-star mask (Vizor's activity rows).
- **Payment sounds** (`docs/sounds.md`): one three-strike signature (E6, B6, E7) cut per
  moment: approve, ready (this approval completed the signatures), sent, received,
  failed. Synthesized by `scripts/sounds/sounds.py` (deterministic; regenerate the app's
  `res/raw/pay_*.ogg` with `scripts/sounds/build.sh`, never edit the Oggs). Played by
  `PaymentFeedback.play(moment, sound:)` → `xyz.zafe/payment_feedback` (`MainActivity`:
  SoundPool, sonification usage, silent unless the ringer is normal; haptic taps in the
  sound's tempo, kept in sync with `PaymentFeedback.taps`). Setting "Payment sounds"
  (`paymentSoundsProvider`, `zafe_payment_sounds`, default on, read in the bootstrap)
  mutes the sound only. Received plays from `ReceivedNotifier.refresh` for new txids that
  are unmined or ≤ 2 confirmations, in the foreground, never on a first load. iOS: no
  handler (haptics only); needs CAF/M4A copies. Payment sounds are a signature, not UI
  feedback: keep them clean, short, open intervals (the user rejected chimes, bass
  "tactile" thuds and bright major-third runs).
- **Balance card + dollars** (2026-10-06, after Vizor): one total (pending included; what's
  pending shows as "Pending" in activity rows, not as notes on the card) and a dollar line
  `fiatText` from `zecPriceProvider`. **Mainnet builds only** (`kShowsFiat`: test coins
  have no value; Vizor does the same), so testnet/regtest never fetch a price. The price
  comes from Rust (`zafe_core::price::zec_usd`, bridge `api/price.rs`) so it follows the Tor
  policy: Tor on → `zcash_client_backend` cryptex (several exchanges, Gemini trusted, own
  isolated circuit `Purpose::Price`); off → one CoinGecko request (needs a User-Agent:
  403 without). Refresh 3 min in the foreground, last price cached 1 h in prefs
  (`zafe_zec_usd_v1`). Live tests: `price::tests::live_direct_price` and the Tor live test.
- **In-app updates** (`services/app_update.dart`, `in_app_update` **4.2.5**: 5.x needs
  Flutter 3.44): Android Play installs only; checks at launch/unlock/resume (6 h apart).
  Priority ≥ 4 (set per release in the Play Console/API) = immediate full-screen update,
  except while a payment is sending; otherwise a background download, then a Home
  "Update ready" card whose Restart is refused mid-send. Debug/sideloaded installs get a
  Play error and nothing shows. Can only be tested end to end with a Play install
  (internal app sharing or an internal testing track).
- **Secret screens** block capture: wrap a route's page in `SecureScreen`
  (`core/platform/secure_screen.dart`, counted) → `xyz.zafe/secure_screen` `setSecure`
  in `MainActivity.kt` (`FLAG_SECURE`). The first Zafe channel with an Android handler:
  `xyz.zafe/haptics` and `window_appearance` have none yet (Dart swallows
  `MissingPluginException`). Check with `adb shell dumpsys window windows | grep SECURE`.
- Settings (`/settings` tab; the vault switcher's "Vault settings" goes there too): vault info
  (name renames locally), signer key,
  hide amounts, theme (`themeModeProvider`, persisted), endpoints (editable, see above),
  open-source licenses (fonts + NOTICE registered in `main.dart`).
  "How it's protected" (`/vault-protection`, `VaultProtectionBody` takes plain data;
  preview `flutter test tool/screens/protection_render_test.dart`): the approval rule, full
  view access, what losing too many keys means, and the post-quantum caveat (spec §2.3).
- **Sync failures** (`core/errors/sync_failure.dart`, pure, `test/sync_failure_test.dart`):
  `homeSyncFailure(syncError:, relayError:)` combines the last wallet sync error
  (`VaultState.syncError`, kept while the next attempt runs so the label doesn't flicker)
  and the last vault refresh error (`ProposalsState.error`) into a `SyncFailureKind`
  (both unreachable = offline). Home's top-nav status shows `statusLabel`; tapping it
  (`MobileTopNav.onSyncTap`) opens `features/home/sync_status_sheet.dart`. Last sync time
  is `VaultState.syncedAt` / `summary.json` `syncedAt` (also written by background
  checks). Classification is typed end to end: `zafe_core::net::NetFailure` reads error
  **types** (rustls error inside `io::Error::get_ref`, `io::ErrorKind::TimedOut`,
  `tonic::TimeoutExpired`/`Cancelled`, reqwest `is_timeout`, non-transport gRPC codes =
  `Server`) into `WalletError::Remote { failure }` / `RelayClientError::Transport
  { failure }`; `wallet::check_server` (run by `sync_vault`) turns `GetLightdInfo` into
  `WrongNetwork` (mainnet vs not only: test networks report names inconsistently) or
  `ServerBehind` (tip + 3 < wallet's known chain tip); `VaultWallet::create` reports a
  birthday above the server tip + 1 as `ServerBehind`. The bridge maps them to
  `ZafeErrorKind::{Network, Tls, NetworkTimeout, ServerBehind, WrongNetwork,
  WalletDatabase}` plus `ZafeError.endpoint` (`Relay`/`Lightwalletd`/`None`). The relay
  client now has a 10 s connect / 60 s request timeout (it had none).
- **Unlock gate** (spec §14; `core/security/device_auth.dart` pure logic behind a
  `DeviceAuthenticator`, `core/security/unlock_gate.dart` `confirmUnlock(context, ref,
  reason:)`, `providers/device_lock_provider.dart`): approving, Send now / Collect
  signatures & send / Try again, proposing, exporting a backup and removing a vault call
  `if (!await confirmUnlock(...)) return;` before acting. Rejecting doesn't. The setting
  "Require unlock to approve" (`zafe_require_unlock`, default on, read in the bootstrap)
  needs an unlock to turn **off**. Biometrics **or** device credential. No screen lock
  (e.g. the emulator) → the action goes ahead, a one-time toast (`zafe_no_screen_lock_warned`)
  and a permanent note in Settings; unknown errors fail closed. Background checks (push,
  WorkManager) answer signing requests and finish auto-sends **without** a prompt: they only
  act on approvals the owner already gave with an unlock. `local_auth` 3.x needs
  `MainActivity : FlutterFragmentActivity` (else `uiUnavailable` → every gated action is
  blocked), `USE_BIOMETRIC`, and iOS `NSFaceIDUsageDescription`.
- **App lock** (spec §14; `core/security/app_lock.dart` pure rules,
  `app_lock_gate.dart` `AppLockGate` = `MaterialApp.builder`, so the lock screen covers
  routes, sheets and toasts while the screen underneath stays mounted with input, focus
  and semantics excluded). Locks at launch when a vault exists and on resume after the
  "Lock app" delay (`zafe_app_lock`: off / immediately / 1 min default / 5 min, read in
  the bootstrap; weakening it needs an unlock). `appLockedProvider` holds the state.
  Gotcha: the device-credential prompt is another activity, so the app goes `hidden`
  behind it; `DeviceAuth.prompting` marks that so unlocking (or a `confirmUnlock`)
  doesn't lock the app again, and a cancelled prompt waits for a tap instead of
  re-prompting. No screen lock on the phone → the app unlocks (Settings warns). It is
  separate from "Require unlock to approve" (per action). Background engines are
  unaffected. **Recent apps**: `MainActivity` calls `setRecentsScreenshotEnabled(false)`
  (Android 13+; screenshots in the app still work) and below 13 sets `FLAG_SECURE` only
  between `onPause` and `onResume` (never clearing one a `SecureScreen` asked for:
  `secureRequested`). That flag doesn't hide the **live** tile Android 13+ shows for the
  running app, so `AppLockGate` also draws `PrivacyCover` whenever the lifecycle isn't
  `resumed` (switcher, notification shade, system dialogs, the unlock prompt). iOS:
  `SceneDelegate` adds a launch-screen cover on `sceneWillResignActive` (untested: iOS
  has never been built). Verified on the emulator 2026-10-04: blank recents card,
  cover gone on return. Emulator test: the
  AVD has no screen lock (the lock then opens at once); `adb shell locksettings set-pin
  1234`, unlock with `adb shell input text 1234` + `KEYCODE_ENTER`, and `locksettings
  clear --old 1234` afterwards. The PIN prompt is a secure window (black screenshots;
  read it with `agent-device snapshot`); one Back cancels it, a second leaves the app.
  Verified 2026-10-04: launch lock, cancel without re-prompt, "Immediately" without a
  relock loop, weakening asks for the PIN.
- **Payment links** (`zcash:`, ZIP 321; "Pay with Zcash" on a site or another app):
  Android intent filter + iOS `CFBundleURLSchemes`; `services/invite_links.dart` reads
  `app_links`' **string** stream (a payment link reaches Rust exactly as delivered) into
  `paymentLinks` (`features/send/payment_link.dart`, latest wins). `app.dart`'s
  `openPaymentLink` waits while locked, during keygen and on `/backup-prompt`, drops a
  link older than `kPaymentLinkTtl` (10 min, Vizor's rule: an old link must not turn
  into a payment on some later unlock) or with no ready vault (toast), then pushes
  `/payment-request` (exempt from the vault redirect: it picks its own vault).
  `PaymentRequestScreen`: "Check who sent this" warning, full address, the link's
  `label`/`message` shown as "(not verified)" (bridge `ScannedPayment.label/message`),
  "Pay from" showing only the chosen vault, with "Change" (a sheet of every vault) when
  there are several (the user asked not to list them all on the page), the approval rule, a funds check
  (`checkFunds`: amount + `minimumFeeZat` = 5,000 × max(2, recipients) against the
  vault's spendable balance when synced this session, else its saved total, which can
  only prove a shortfall; a vault that's short is marked "short by X", can't be picked,
  and the default moves to one that can pay; nothing is built, so the real fee can be
  higher), and Continue
  only after "I know who sent this..." is ticked; Continue switches vault and opens
  `/send` prefilled (`SendPrefill.fromLink`: review step, or amount when the link has
  none). Proposing still takes `confirmUnlock` and t approvals. Test:
  `adb shell "am start -a android.intent.action.VIEW -d 'zcash:<ua>?amount=1&label=Shop'"`.
  Preview: `flutter test tool/screens/payment_request_render_test.dart`. Verified on the
  emulator 2026-10-04: cold start through the lock, warm link, Continue → Review
  ("PAYMENT REQUEST"). Full e2e 2026-10-04: two vaults made with CLI members (B1/C1,
  B2/C2 + the app; 15 and 5 ZEC), link for 2 ZEC, picked the non-active vault, proposed,
  app + C2 approved, interactive send (C2 `respond`), recipient +2, payer −2.0001 (fee
  10,000 zats = the shown minimum). Backing out to the launcher ends the activity, so a link still
  waiting behind the lock is gone when the app is reopened from the launcher.
- **Backups** (spec §12.2; `zafe_core::backup`, bridge `api/backup.rs`, app `features/backup/`):
  `ZAFEBAK` v2 = header (Argon2id params, salt, nonce; authenticated as AEAD data) +
  XChaCha20-Poly1305 of {identity seeds, material, invite, signer names}; **never nonces**
  (nor the wallet DB key: a restore gets a new key and resyncs). v1 (no names) still
  restores. Restore writes the names to `names.json` (cleaned with `MemberNames.clean`,
  non-hex keys dropped) before the vault becomes active. Import checks
  the identity is a vault member and refuses KDF params below 64 MiB / 3 passes (or absurdly
  high). Text form `zafe-backup-v1:` + base64url. Passphrase: 12+ words or zxcvbn 4
  ("Suggest" = 12 BIP-39 words). The app prompts right after key generation
  (`/backup-prompt`) and shows a home reminder until `summary.json` has `backedUp`
  (set by export, and by restoring). CLI: `zafe backup --passphrase …` prints the text form.
  Testing tip: this emulator has no shell clipboard; paste into a field (e.g. Settings
  search) and read it with `agent-device get text @ref` (snapshots truncate long values).
- **Multiple vaults**: `ZafeSecureStore` keeps each vault's identity, invite and material
  under `zafe_vault_<vaultId>_*`, listed in `zafe_vaults`; the active vault id is in prefs
  (`zafe_active_vault`). Each vault gets a **fresh member identity** (the relay can't link
  memberships). Per-vault files live in `ZafePaths.vaultDir(id)`: `signing/` (pass
  `await paths.stateDir(id)` to Rust), `seen.json`, `summary.json` (switcher's balance and
  pending count, written by the app and background checks). Wallet DBs stay
  `vault-<id>.sqlite` in the support dir. `VaultState` exposes the **active** vault
  through the old getters (`identity`, `material`, `hasVault`...); `ProposalsNotifier`
  rebuilds when `activeId` changes. The pre-multi-vault layout is migrated in
  `VaultBootstrap.load()`. Notification payloads are `vaultId:proposalId`; a tap switches
  vaults first. The switcher opens from the vault name on home.
- **Notifications** (`lib/src/notifications/`): the relay pushes every other member on
  each log append (content-free; FCM data message, high priority, one collapse key). A push,
  the 15-minute WorkManager task, or a one-off check a minute after the app is backgrounded
  all run `checkVaultAndNotify()`: sync, list, answer interactive requests, finish an
  auto-send this member owes, then `vaultUpdates()` (pure, unit-tested) diffs against the
  last-seen snapshot (`<appSupport>/notifications/seen.json`, a file because prefs caches
  per isolate) and shows local notifications; the app records the snapshot on every
  refresh so nothing seen in-app is re-announced. Tapping opens `/proposal/:id`.
- **Notification icon**: `res/drawable/ic_notification.xml`, a vector of the icon's Seam mark
  written by `scripts/brand/brand.py` (`write_notification_icon`); Android uses only
  its alpha, so a full-colour launcher icon shows as a filled square. Used by
  `flutter_local_notifications` (init + `icon:`, tinted `#00736C`, also `@color/zafe_notification`) and as FCM's
  `default_notification_icon`. `res/raw/keep.xml` keeps it through release resource
  shrinking (Dart refers to it by name only).
- **FCM is opt-in per build**: drop the Firebase project's `google-services.json` into
  `app/android/app/` (gitignored) and the Gradle plugin applies itself; without it the app
  builds and relies on background checks. Relay: `ZAFE_FCM_SERVICE_ACCOUNT=<key.json>`
  (never commit it). APNs isn't implemented yet (iOS).
- **Firebase project `zafe-18c4d`** (the user's account), Android apps `xyz.zafe.zafe`
  (`1:303423821426:android:deff491a8e80f394ea14eb`) and the testnet app
  `xyz.zafe.zafe.testnet` (`1:303423821426:android:8a429ba817be26f5ea14eb`, added
  2026-10-06); one `google-services.json` lists both. Regenerate the app config with
  `npx -y firebase-tools@latest apps:sdkconfig ANDROID <app id> --project zafe-18c4d --out
  app/android/app/google-services.json`. The relay key (service account
  `firebase-adminsdk-fbsvc@zafe-18c4d.iam.gserviceaccount.com`) lives at
  `~/.config/zafe/fcm-service-account.json` (0600, outside the repo); it was created
  through the IAM API with the Firebase CLI login token (the CLI has no command for it).
  The CLI login in a non-interactive shell is two steps: `login --no-localhost`, then
  `login <code>`.
- **FCM on the emulator is slow** (~2 min from send to delivery on `google_apis`), so
  don't conclude "not delivered" too early: check `adb logcat | grep FLTFireMsg`. The
  relay logs `push: sent` at debug level (`RUST_LOG=zafe_relay=debug`).
- **Invites** (`features/onboarding/invite_link.dart`, pure and unit-tested): raw form
  `zafe-invite-v1:<hex postcard>` (~250 chars; URL-safe, asserted in `node::tests`), link
  form `zafe://join?invite=<raw>`, or `https://<ZAFE_LINK_HOST>/join#<raw>` when the build
  has the dart-define (invite in the **fragment**, so the landing site's server never sees
  it; only that exact host, port 443, no query). Both forms are always accepted; the https
  one only for the configured host. **Never default `ZAFE_LINK_HOST` to a domain we don't
  control**: that site's page script could read every invite. The setup QR and "Share link" carry the link; Join
  accepts either (typed, pasted inside a message, or scanned on `/scan-invite`). **An
  invite is a bearer credential until membership is locked**: its `join_token` lets anyone
  take an empty seat (it can't spend or see funds; the creator's member list and the
  safety number catch an intruder). Copy says to share it only with co-signers over a
  trusted channel. Links never join by themselves: they open Join prefilled, and the
  safety number flow follows.
- **Deep links**: `app_links` 7.0.0 (7.1+ needs Flutter 3.44), with Flutter's own deep
  linking **off** (`flutter_deeplinking_enabled` meta-data / `FlutterDeepLinkingEnabled`),
  otherwise go_router receives `zafe://join` itself. **App Links**: a second, `autoVerify`
  intent filter for `https://<host>/join`; Gradle decodes Flutter's `dart-defines` project
  property to put `ZAFE_LINK_HOST` in the `zafeLinkHost` manifest placeholder (unset →
  `links.zafe.invalid`, which matches nothing; a malformed host fails the build). Check the
  merge without a full build: `flutter build apk --config-only`, then
  `JAVA_HOME=~/android/jdk-17 android/gradlew -p android :app:processDebugMainManifest
  -Pdart-defines=<base64 of NAME=value>`. Landing site + `assetlinks.json`/AASA:
  `infra/site/` (README). iOS Associated Domains not added yet (iOS has never been built;
  the entitlement breaks signing without a team). `services/invite_links.dart` feeds
  `inviteLinks` and `paymentLinks` (its stream also delivers the launch link); `app.dart` opens
  `/welcome` + push `/join?invite=` (calling `beginAddVault` when a vault is active) and
  defers while keys are being made or on `/backup-prompt`. Test on a device with
  `adb shell "am start -a android.intent.action.VIEW -d 'zafe://join?invite=zafe-invite-v1:...'"`
  (quoted twice: the device shell splits it again; cold start: `adb shell am force-stop
  xyz.zafe.zafe` first).
- **Website** (`infra/site/`, Astro 7, static): a calm home page `/` (no JS; structured
  like safe.global, `lp-*` classes), the 3D story on `/showcase` (keep heavy motion off
  the home page: the user's call, a treasury product should feel sober), and the invite
  page `/join`. **The two pages must not look alike** (round 12): home shows the app's
  real screens in phone frames (`.lp-phone`) and only one still of the world (the
  `/showcase` teaser); the showcase has no logo intro, a fixed `light` nav (`Nav.astro`),
  a title card and a slim footer. Nav links marked `mobile` are the only ones on phones.
  Phones (2026-10-01 fixes): `body`'s `overflow-x: clip` doesn't stop the viewport
  scrolling sideways, so clip wide decorations on their section (`.lp-hero`) and check
  `scrollWidth == clientWidth` at 390 px; the viewport is `viewport-fit=cover` (Chrome
  on Android draws edge to edge) and footers pad `env(safe-area-inset-bottom)`; the 3D
  world starts phones at dpr 2 + MSAA 2 and stops adapting when a drop doesn't speed
  frames up (a 30 fps cap, not the GPU).
  The whole design/build/verify/ship flow is the project skill `.claude/skills/landing-page`
  (`frames.sh` + `sheet.py` capture a scroll section frame by frame into a contact sheet).
  Chose Astro over Next.js (Vizor's site) to keep a strict CSP with no inline code: the
  landing page bundles its motion (`src/scripts/story.js` → `world.js` Three.js world,
  `ui.js` pointer and text) as same-origin modules; `/join` loads only the hand-written
  `public/assets/join.js` (`<script is:inline src=…>`, never bundled; it's the only code
  that sees the invite). agent-browser reports no fine pointer, so check the custom
  cursor with `/showcase?pointer=fine`; the 3D world needs a few seconds after each scroll
  jump before a screenshot. agent-browser renders WebGL in software, and the page skips
  the world on software renderers (and gives up at runtime if it stays under ~20 fps at
  its lowest quality), so screenshots of the world need `/showcase?world=always`.
  Cache headers live in `public/_headers` (`/assets/...` blocks; `vercel-output.sh`
  turns each into a Vercel route). The **static fallback** (no GPU, reduced motion, no
  JS) shows stills of the world rendered by `infra/site/stills.sh` (capture mode
  `/showcase?still`; also writes the social image `og.png`); re-run it after changing the world
  or the story. `public/assets/boot.js` adds `html.js` before the first paint so the
  world's visitors never fetch the stills; anything only for the fallback goes under
  `:is(html:not(.js), html.stills)`.
  **SEO** (`docs/seo.md`): the public origin is one value, `site` in `astro.config.mjs`
  (`https://zafe.cash` since 2026-10-01: Cloudflare DNS, DNS-only CNAMEs to Vercel, www
  redirects to the apex; mail to any `@zafe.cash` address is forwarded by Cloudflare
  Email Routing); canonical, og:url/og:image, `sitemap.xml`, `robots.txt` and
  `.well-known/security.txt` (Astro endpoints in `src/pages/`; security.txt's `Expires`
  is build + 1 year, so redeploy at least yearly) derive from it. `build.sh` validates
  only the two JSON files in `.well-known`. Registered in Google Search Console and Bing
  Webmaster Tools (DNS TXT); each production deploy submits the sitemap's URLs to
  IndexNow (`indexnow.sh`, key file `public/<key>.txt`, public by design; failures only
  warn).
  Head tags live in `Base.astro` (props `title`, `description`, `socialTitle`, `jsonLd`,
  `noindex`); social image `/assets/og.png` (1200×630) in `config.ts`. `/join` stays
  `noindex`, out of the sitemap and **not** disallowed in robots.txt (Google must fetch
  it to see the noindex). JSON-LD on `/` (Organization, WebSite, MobileApplication,
  FAQPage from the same `faq` array as the page) is the one inline `<script>`
  `build.sh` allows: exactly `type="application/ld+json"`, no other attribute, contents
  must parse as JSON. Raster icons + manifest: `icons.sh` (CSP has `manifest-src 'self'`).
  `astro.config.mjs` sets `inlineStylesheets: 'never'` and `build.format: 'file'`;
  `build.sh` greps the output and fails on any inline script/style/handler. Phone shots
  are the app's own renders (`tool/screens/*_render_test.dart` → `infra/site/assets.py`
  → WebP); fonts are the app's, subset to WOFF2. Calls to action go through `getUrl`/`getLabel`/`getNote` in
  `src/config.ts`: "Get Zafe" only when the build sets `ZAFE_DOWNLOAD_URL`, otherwise
  "View code" to the repo (no APK testers can use before the relay is hosted). Landing copy must only claim shipped
  features. Look at it with agent-browser at 1280 and 390 wide, light and dark.
  **Design** follows `docs/site.md` (now laid out after vizor.cash; decisions at the top): teal only
  for the action and "needs you", gold only for amounts, Space Grotesk only at display
  sizes, real app crops and illustrations (`assets.py` CROPS; wide pillar art is padded to 5:4). The
  chain-view numbers are a real testnet transaction: never replace them with invented
  ones. Patina icons are inlined by `src/components/Icon.astro` (magenta → 38% layer).
  **Deploy**: `.github/workflows/site.yml` → Vercel with `vercel deploy --prebuilt` on
  `.vercel/output` from `vercel-output.sh` (Build Output API v3; headers read from
  `public/_headers`, `overrides` serve `join.html` at `/join`). `builds.json` isn't
  needed (the CLI only reads it for `vercel build` errors). Vercel never builds.
- **CI disk**: the Rust job's test binaries (arti, Halo 2, SQLCipher linked into each)
  filled the runner disk and `ld` died with `signal 7 [Bus error]`. `ci.yml` frees ~30 GB
  first and builds with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`. A bus error or "No
  space left" in a link step means disk, not code.
- **Scanner**: `mobile_scanner` 7.4.2 with our own controller, so `scan_invite_screen.dart`
  handles lifecycle itself (stop on inactive only while running, because the permission
  prompt makes the app inactive; `start()` on resume also picks up a permission granted
  in Settings). `permission_handler` is only used for `openAppSettings()`.
  It bundles ML Kit's barcode model on Android (adds a few MB to the APK).
- `flutter analyze` in a fresh worktree reports errors in
  `rust_builder/cargokit/build_tool` until `dart pub get` runs there (not our code).
- Font family names in code must match `pubspec.yaml` exactly (`Space Grotesk`, `DM Sans`,
  `JetBrains Mono`).
- `pubspec.yaml` must have a single `flutter:` key (a duplicate silently breaks FRB codegen).
- Cargokit is patched (`rust_builder/cargokit/gradle/plugin.gradle`): debug builds no longer
  add x86/x64 unless `-Pzafe.debugEmulatorAbis=true`. Build for a phone with
  `flutter build apk --debug --target-platform android-arm64` (~5 min cold).
  Stale emulator `.so` files can linger in `build/rust_lib_zafe/jniLibs`; delete them.

Device testing (agent-device): emulator AVD `zafe` (API 35 x86_64; needs `/dev/kvm`
access and `libxkbfile.so.1`, which was unpacked into `~/android/sdk/emulator/lib64` without
root). Start it windowed with `emulator -avd zafe -gpu swangle_indirect -no-snapshot
-no-audio`; build with `--target-platform android-x64`. `scripts/app-harness.sh` runs the
relay plus CLI members B and C and sets `adb reverse` for 8787/9067, so the app's localhost
defaults work on a device. Its `cli` subcommand does not rebuild: `cargo build -p zafe-cli`
after core changes. **Emulator renderer (WSL)**: `-gpu swiftshader_indirect` segfaulted in the host
`RenderThread` three times on 2026-09-30 (`dmesg`: "RenderThread: potentially unexpected fatal
signal 11"), around the proposal/sending screens; the emulator died, not the app. Use
`-gpu swangle_indirect` (stable since). AVD RAM raised to 4G for proving. A send's broadcast
can still complete when the emulator dies mid-send: check the vault log (`zafe proposals`)
before retrying. agent-device tips: prefer `find "<text>" click`; refs go stale after
every snapshot; `scroll down --until 'label="..."'` before pressing bottom buttons.

In a fresh git worktree, plain `flutter analyze` reports ~50 errors in
`rust_builder/cargokit/build_tool` until `dart pub get` runs there; `flutter analyze lib
test` checks the app alone.

**Release APKs** (`docs/releasing.md`, `.github/workflows/release.yml`): always
`--split-per-abi` with one `--target-platform`. Plugins (ML Kit) bundle
armeabi-v7a/x86_64 libs our Rust library lacks, so an unsplit APK installs on those
devices and crashes (`libflutter.so is for EM_AARCH64`), and Flutter's Gradle plugin
clears any `buildTypes.*.ndk.abiFilters` you set. Signing reads `android/key.properties`
(gitignored); without it release builds use the debug key.

App commands (from `app/`, after `source ~/android/env.sh`):
`flutter_rust_bridge_codegen generate` (after changing `app/rust/src/api`), `flutter analyze`
(must be clean), `flutter build apk --debug --target-platform android-arm64`,
`adb install -r build/app/outputs/flutter-apk/app-debug.apk`.

Toolchain (installed by `~/android/install-toolchain.sh`; `source ~/android/env.sh`): Flutter at `~/flutter`, JDK 17 at
`~/android/jdk-17`, Android SDK at `~/android/sdk` (platform 36, build-tools 36.0.0).

## Upstream status (check before relying on it)

- **Upstream gates cleared (2026-10-01, frost#1094, conradoplg)**: U1 (vaults derived per
  ZIP 2005 § 4.2.3 from our own `sk` agreement: change "very unlikely", "if you have your
  own system that will also work"), U5 (builder-chosen `alpha` fine; keep it secret) and U3
  (redpallas moves to the FROST repo, same serialization). Mainnet now waits on the external
  audit (spec M2); still testnet/regtest only until then. Daira-Emma Hopwood confirmed
  on Discord (2026-10-01): U5 "fine"; a vault with `ak` from the DKG and `nk`/`qsk`/`rivk_ext`
  from `sk` with `use_qsk = true` is "the intended usage"; `use_qsk = false` does not
  conform to ZIP 2005 and is not quantum-recoverable.
- **Contributing to frost-tools**: fork at `zafe-cash/frost-tools`; PRs are squashed, so
  the PR title must be a Conventional Commit; no PR template; coordinate larger changes in
  an issue first. CI = `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test` (all quick). Deps go through `[workspace.dependencies]`.
  `zcash-sign` still uses the fork's `from_sk_ak_incompatible...` (conradoplg: ignore
  quantum recoverability for now, frost-tools#591). Our vectors: #609 / #610.
- **Post-quantum**: every member holds `qsk`, so a discrete-log-breaking adversary needs
  only one member's `qsk` (ZIP 2005 says so; spec §2.3). Not fixable inside ZIP 2005.
- COCKTAIL-DKG (frost#1033) not production-ready; Zafe uses frost-core DKG + own echo/transcript.
  The zips#895 draft specifies `sk` agreement as COCKTAIL-DKG payloads,
  `sk = H(n ‖ len(payload_1) ‖ payload_1 ‖ …)`; Zafe's own scheme differs. Align when
  COCKTAIL-DKG ships with Pallas (the WIP frost#1032 has none).
- RedPallas FROST ciphersuite moving out of `reddsa` (frost#963); stay on 0.5.2 until then.
- **Changing t** (frost#1082, open, not a ZF priority): `frost-core` refresh keeps
  `min_signers`; resharing (Desmedt-Jajodia/GRR98) works on redpallas with the `internals`
  feature (`zafe-core/tests/reshare.rs`, a dev-dependency only; spec §10.4.4). Not shipped:
  own crypto, needs spec + audit and one-time ceremony keys first. Kept old shares always
  still sign at the old t.

## Machine hygiene

- **Never run `docker system/volume/image prune` on this machine**: other projects' data
  live in Docker here. Remove only the containers/volumes/images you created, by name.
- `scripts/app-harness.sh` keeps its state in `~/.cache/zafe-harness` (not `/tmp`, which
  a WSL restart wipes); after a restart run `scripts/app-harness.sh resume`.

## User preferences

- Research primary sources (repos, ZIPs, issues) before asking the user or relying on memory;
  Zcash moves fast (Ironwood/NU6.3 postdates older knowledge).
- Record durable learnings here as you go.

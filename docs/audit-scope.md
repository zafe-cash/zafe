# Zafe: external audit scope

For the auditors, and for us to check that nothing else stands between the code and
mainnet. Written 2026-10-07 at the end of the "before mainnet" work; updated 2026-10-27
for per-vault relays, labels, relay moves and relay expiry (§1-§7). The commit to audit
is the one the audit engagement letter names (record it in `docs/tracker.md`). Product
spec: `spec.md` (protocol §5-§10, security model §13). Invariants and gotchas:
`AGENTS.md`. Questions to the Zcash Foundation and their answers: `upstream-asks.md`.
Relay moves in depth, with their own security review: `docs/relay-move.md` §8.

What Zafe is, in two sentences: a Safe-style shielded multisig for Zcash. A vault is an
Orchard-type (Ironwood pool) account whose spend authority is a **re-randomized FROST**
(t-of-n, RedPallas) key, with quantum-recoverable viewing keys per **ZIP 2005**; members'
phones coordinate through a **blind relay** (the app calls it the "server") that only
sees ciphertext and public keys.

## 1. What to audit, in priority order

Lines are Rust, `wc -l` of the files named (2026-10-27; paths under `crates/zafe-core/src`
unless shown). **P0 = loss or theft of funds if wrong.**

| Pri | Area | Files | What can go wrong |
|---|---|---|---|
| P0 | Vault keys (ZIP 2005 `use_qsk = true`) | `keys.rs` (326), `tests/zip2005_vectors.rs`, `scripts/check_zip2005_vectors.py` | wrong derivation = unrecoverable or unusable vault; vectors are also cross-checked in Python and against frost-tools |
| P0 | DKG, `sk` agreement, safety words | `keygen.rs` (432), `sas.rs` (136), `node::{keygen_step, run_keygen}` | a member (or the relay) biasing or learning `sk`/the group key; echo, commitments to `sk` contributions, HPKE sealing; a relay that shows members different member lists and still gets matching words |
| P0 | Independent verification of a PCZT | `verify.rs` (290), `tx.rs` (158) | a member approving or signing a transaction that pays anyone but the proposed payees; fee, change, spends, sighash |
| P0 | FROST signing, randomizer, nonces | `signing.rs` (89), `session.rs` (643), `nonce_store.rs` (367), `node::{approve, sign_own_shares, send_ready, finalize}` | nonce reuse (key leak), signing a different message, accepting a bad share, `alpha` leaking |
| P0 | One-tap signing and replay determinism | `vault.rs` (1292): `assign_commitments`, `apply_versioned`, `ProposalState`; `node::{top_up_pool, forget_closed, propose_again}`, `PoolReset` | members computing different states; a commitment assigned twice; shares for a group the member isn't in |
| P0 | Note reservation, sweeps, expiry | `vault.rs` (`NotesInUse`), `node::{reserve_notes, note_holds, invalidate}`, `wallet.rs` | double-spending a note into two payable proposals; a cancelled but fully signed transaction staying sendable |
| P1 | Relay-facing logic that guards against a hostile relay | `node.rs` (4439: `load_log`, `catch_up`, `reseed_relay`, `install_log_copy`, `check_relay_membership`, `follow_relay`), `log_cache.rs` (228), `zafe-proto/src/{log,envelope,identity}.rs` | a relay forging, forking, rolling back, reordering or replaying; signature, AEAD and hash-chain checks; a first load that isn't anchored |
| P1 | Relay moves, labels, create token | `node/relay_move.rs` (1269), `moves.rs` (344), `vault.rs` (`VaultMoved` replay: arrivals, base hash, order), write rules in `node::append_event` (`write_target`: move notices read before each write), `zafe-relay` labelled routes (`/v1/log/append-labelled`, `/v1/mailbox/replace-labelled`, stored `vault_id`) and `charge_creation` (create token) | a switch with fewer than m = max(t, n − t + 1) arrivals or to a relay a member didn't tap; two switches from one base; an entry on the new relay before the switch; vault id vs label mix-ups; a squatted mailbox; the create token leaking into invites or seat links |
| P1 | Membership change and share repair | `repair.rs` (998), `vault.rs` seat moves, relay `replace_member` / `replace-labelled` | taking over another member's seat; leaking or corrupting a share during repair (RTS); a recovering phone trusting a log its helpers' anchors don't match |
| P1 | Backups | `backup.rs` (472) | weak KDF, nonces in a backup (never allowed), restoring an identity that isn't a member, a log copy that doesn't verify |
| P1 | Wallet and secrets at rest | `wallet.rs` (SQLCipher open, key handling), app `core/storage`, `core/security` | key in the wrong place, DB opened unencrypted, unlock gate bypass |
| P1 | Relay server | `zafe-relay/src/lib.rs` (1943), `limits.rs`, `quota.rs`, `wait.rs`, `expiry.rs` | auth of every route, quota/rate bypass, SQL, a member reading or writing another vault's mailbox, `reseed`, the create token, expiry deleting a live mailbox |
| P2 | App | `app/lib`, `app/rust/src/api` | UI states that show unverified data; bridge errors; deep links (`zcash:`, invites) |
| P2 | Tor, TLS, price | `tor.rs`, `net.rs`, `relay_client.rs`, `price.rs` | traffic leaving outside Tor when Tor is on; certificate handling |
| P2 | Infra | `infra/relay/**`, `.github/workflows/relay-*.yml` | deploy path; secrets handling; the digest promoted to mainnet |
| P2 | Unapproved-spend alert | `spend_watch.rs` (78), `wallet.rs::vault_spends` | false negatives (a theft the alert misses) |

Not in scope: Flutter and plugin internals, `zcash_client_backend`/librustzcash,
`frost-core`/`reddsa`/`orchard` (we pin them, see §6; we ask for a review of **how** we use
them), the landing site (`infra/site`), brand assets, scripts that only build art.

## 2. Threat model

Assets: vault funds; privacy of balances, payees and amounts (viewing keys); availability
of the approval flow (nothing may brick funds, nothing may stop members from spending).

| Adversary | Can | Must not |
|---|---|---|
| Relay operator or a network attacker | see metadata (who, when, sizes, IPs, mailbox ids, push tokens); drop, delay, reorder, replay messages; serve stale, forked or shorter logs; refuse service | read content; forge a member's message, vote or log entry; make a member sign anything the member has not verified; make a device accept a log shorter than the one it saw (device copy, spec §6.3); learn `sk`/shares; switch a vault to another relay (needs m members' signed arrivals, spec §6.4) |
| One or fewer than t malicious members | read everything (they hold `sk`); propose anything; refuse to approve; try to confuse others with odd logs or messages | spend; make another member sign a transaction that fails that member's own check; reuse another member's nonce |
| The proposer or the leader (a member) | choose the PCZT and `alpha`; choose who completes a signing round | change payees, amounts, memos, fee, change destination, or the sighash another member signs; learn more than the signature shares |
| t or more colluding or compromised members | spend, bypass rules | (inherent: spec §13.1; detected after the fact by the unapproved-spend alert) |
| A compromised phone | use that member's share while unlocked or whenever the app can sign without a prompt (background auto-send finishes only approvals the owner already gave) | read the share from backups (`allowBackup=false`, nonces never in backups); sign without the unlock gate (an unlock is always required to approve, propose or send) |
| A removed member who kept an old share | sign at the old threshold (spec §10.4.4; fixable only by migrating) | (detected by the unapproved-spend alert) |
| A quantum adversary (future) | break discrete log: with one member's `qsk` recover spend authority to a vault (spec §2.3, ZIP 2005) | (accepted; not fixable inside ZIP 2005) |
| Malware or another app on the phone | read app-private storage on a rooted device | (out of scope beyond OS sandboxing, SQLCipher, secure storage) |

Explicit trust assumptions: members compare the safety words (4 BIP-39 words from a
commit-then-reveal exchange) out of band during setup; OS secure storage and RNG work; upstream crypto crates are correct (§6); GitHub Actions
and the Docker host are trusted to deploy the relay (the relay is blind, so this is a
privacy and availability risk, not custody).

## 3. Protocol invariants (breaking one is a funds or security bug)

The full list is in `AGENTS.md` "Protocol invariants"; this is the audit checklist, with
the code that enforces each and the test that should catch a regression.

1. **Vault keys.** `ak` from the DKG; `nk`, `qsk`, `qk`, `rivk_ext` from the agreed `sk`
   exactly as ZIP 2005 § 4.2.3 with `use_qsk = true`. Never frost-tools'
   `from_sk_ak_incompatible_with_quantum_recoverability`. `keys.rs`; `zip2005_vectors`,
   `keygen::members_agree_on_vault_keys`.
2. **DKG integrity.** Safety words confirmed first; round-1 echo hashes equal across
   members; each `sk` contribution committed in round 1 and revealed after (`Round1Msg.
   sk_commitment`); a mismatch aborts; round-2 packages and contributions HPKE-sealed;
   every member signs the descriptor. `keygen::{equivocating_relay_is_detected_by_echo,
   sk_contributions_are_committed_first, a_contribution_changed_after_seeing_the_others_is_rejected}`.
3. **What gets verified before any signature.** v6 + branch id + expiry; only Ironwood
   actions; every spend is a vault note or a zero-value pre-signed dummy (all spends before
   any output); every payment output recovered with the vault **external OVK** and matched
   exactly (recipient, amount, memo); change belongs to the vault and decrypts with its
   IVK; fee equals ZIP 317 (`5000 * max(2, actions)`); sums use checked arithmetic;
   the sighash is computed locally. `verify.rs`; `tests/verify.rs` (9 negative cases).
   Verification runs again at signing time (`Member::sign`, `sign_groups`).
4. **What to sign.** Every Ironwood action whose `spend_auth_sig` is `None`, never filtered
   by value; any unsigned Orchard-pool spend is rejected. `tx::spends_to_sign`.
5. **Randomizer.** `alpha` is the PCZT's own, fixed before round 1 (deviation from ZIP 312,
   see §4); it never leaves members (PCZTs exported outside the vault must strip it).
   Taken from the member's own verification, never from a request: `Member::sign`.
6. **Nonces.** One pair per (proposal, PCZT hash) or per pool commitment; every package or
   assigned commitment is checked against the stored commitments **before** any nonce is
   taken; nonces are deleted before a share exists; written atomically **before** the
   commitment is published; never in backups. `session::{approve, sign, sign_groups}`,
   `nonce_store.rs`; `session::{nonces_are_single_use, bad_request_does_not_burn_nonces}`,
   `one_tap::{nonces_are_single_use, a_bad_plan_consumes_nothing,
   a_mismatching_transaction_consumes_nothing}`.
7. **Shares are bound to the exact request** (request hash); aggregation always goes
   through `session::aggregate_request` / `aggregate_group` (signer set + per-share
   verification, a bad share is attributed).
8. **Replay is deterministic, lenient after creation, fatal only at creation.** Every
   member computes the same `VaultState` from the same log; invalid entries go to
   `ignored`; `Created` must be first and signed by all members. Rules that changed over
   time are keyed by the event version the author's app wrote
   (`VaultState::apply_versioned`): for example version 6 assigns one-tap commitments per
   fully covered group, versions 1-5 keep all-or-nothing. Every version and its gate is
   listed in `AGENTS.md` (`VAULT_EVENT`, now 10). `tests/vault.rs` (40 tests).
9. **Note reservation (two layers).** Log rule: a proposal that spends a note of an open or
   approved proposal that hasn't expired is ignored (`NotesInUse`); wallet holds lock
   those notes in the wallet DB until expiry. Cancelling after signatures are complete is a
   respend ("sweep"), which invalidates the old transaction.
10. **Expiry is never removed**: members accept a window + slack only; a complete one-tap
    group stays sendable until expiry.
11. **The relay is untrusted for ordering and memory.** Clients check every entry's
    signature, index, hash chain and (for envelopes) per-sender sequence numbers; they
    drop envelopes from non-members. A device keeps its own copy of each vault's log and
    refuses a relay that no longer holds its last entry (`RelayRolledBack`, `RelayForked`,
    `RelayLostVault`). A restore installs the backup's log copy before its first load, and a
    recovering phone checks its helpers' log anchors. `node::load_log`, `log_cache.rs`;
    `tests/relay_rollback.rs`.
12. **Versioned formats.** Every persisted or wire format has a tag; decoders accept only
    what they know and fail with `UnsupportedVersion` (never garbage); version bumps and
    their gates are recorded in `AGENTS.md`.
13. **Wallet DB encrypted** with a per-vault raw key derived from the identity seeds and
    vault id (`WalletKey::derive`, nothing stored); the open path
    refuses to run if SQLCipher isn't linked; a wrong key resyncs, never opens plain.
14. **Backups never contain nonces or the wallet key**; the KDF floor is 64 MiB / 3 passes.
15. **Relay authentication.** Every state-changing or reading route verifies a signature,
    charges the key's rate limit only after it verifies, and checks membership; reads and
    `reseed` carry a timestamp (±300 s). Quotas keep running byte counters updated in the
    same transaction as the write. The create token (`ZAFE_RELAY_CREATE_TOKEN`) is checked
    only when a mailbox is created or reseeded; expiry (`zafe_relay::expiry`) deletes a
    mailbox and every row stored for it.
16. **Relay moves and labels.** A vault switches relay only through a `VaultMoved` entry
    carrying m = max(t, n − t + 1) distinct current members' arrivals over one move target;
    replay checks each arrival, the base hash and the order; two switches from one base
    are followed by nobody. Nothing connects to a new relay before the member's tap, and
    no new entry is written there before the switch. The vault id binds keys and entries;
    the label only addresses the relay. `node/relay_move.rs`, `moves.rs`; `tests/relay_move.rs`
    (16), `tests/relay_labels.rs`, relay `tests/labels.rs`. Design: `docs/relay-move.md` §4.

## 4. Known deviations, accepted risks and open questions

| Item | Status |
|---|---|
| **ZIP 312 randomizer.** ZIP 312 (zips#895 draft) says the Coordinator derives the randomizer after round 1 from fresh bytes and the commitment list. Zafe uses the builder's `alpha` (fixed before round 1) because one-tap commitments exist before any proposal and several signer groups share one `alpha` per spend. Security: Re-Randomized FROST (ePrint 2024/436) proves unforgeability for adversarially chosen randomizers; `alpha` is public to signers. Confirmed acceptable by ZF (conradoplg, frost#1094, 2026-10-01) and Daira-Emma Hopwood (Discord); requirement: `alpha` links `rk` to `ak`, so it must stay within the vault. Spec §9.5.1; asked as U5. | accepted |
| **Deprecated `frost_rerandomized::sign`** used in one wrapper (`signing.rs`) because it is the only external-randomizer API; ZF says an external-randomizer API will stay (U2). | accepted |
| **`sk` agreement is our own scheme**, not COCKTAIL-DKG (frost#1033, not production-ready, no Pallas). ZF: "if you have your own system that will also work" (U1). Align when COCKTAIL-DKG ships. | open upstream |
| **Post-quantum:** every member holds `qsk`, so one member's `qsk` + a discrete-log break is enough to recover spend authority; ZIP 2005 says so. Not fixable inside ZIP 2005. | accepted, told to users (app: Vault protection screen; site: `/docs/security`) |
| **Messaging crypto is the previous generation** (`ed25519-dalek` 2, `hpke` 0.12, `chacha20poly1305` 0.10): the newer set needs stable `sha2` 0.11, which conflicted with `bip32`'s pre-release `sha2` pin under `zcash_client_backend` 0.24. The NU7 line resolves `bip32` 0.6 on `sha2` 0.11, so the upgrade can be revisited (spec §4.4). | accepted until upgraded |
| **Ceremony and repair messages are sealed to long-term identity keys** (no forward secrecy): someone who recorded them and later steals an identity key learns what those messages carried. One-time ceremony keys are planned before refresh/reshare ships (spec §10.4.4). | accepted for v1 |
| **First load from an old backup trusts the relay once.** Since 2026-10-25 a restore installs the backup's log copy (`BACKUP` 3) and a recovered seat checks its helpers' anchors (`REPAIR` 3) before the first load. A restore from a version 1 or 2 backup has no copy and can't detect a relay that serves it a shortened or forked log on that first load. | accepted; spec §6.3 |
| **A relay restored from an old database can lose in-flight messages and push tokens** until members ask again. Funds and the vault log are unaffected (members restore the log). | accepted |
| **A hostile relay can always deny service.** Members move the vault to another relay (spec §6.4); a move works with the old relay dead. Labels stop anyone who knows a vault id from claiming the vault's mailbox on another relay first (`docs/relay-move.md` §4.6). | accepted |
| **Rules, address book, rotation, resharing, migration** are not implemented (spec §10-§11, M3): a vault can't change members or `t` except by seat moves; a removed device keeps its share. Mainnet beta ships without them. | out of scope for v1; the unapproved-spend alert covers old shares |
| **Unapproved-spend alert has a grace window** (24 blocks) in which a spend that spends a logged proposal's notes isn't flagged; a thief who only re-signs an existing proposal's notes to another address is flagged after the window, not at once. | accepted |
| **iOS has never been built.** Backup exclusion, Notification Service Extension and background behaviour are untested. Android only for the beta. | out of scope |
| **Android background checks sign nothing new**: they answer interactive requests and finish auto-sends the owner already approved, without an unlock prompt, by design. | accepted |
| **One relay per vault at a time.** The relay orders the log; members move the vault with a relay move (spec §6.4, event version 10). | by design |
| **The mainnet beta has no cap**, only a "beta" label in the app (decided 2026-10-07): no limit on vaults per relay or on what a vault holds. Per-IP/per-key creation limits stay as anti-abuse. | by design |

## 5. Test inventory

Run everything the CI runs (`AGENTS.md` "Commands"). Counts are `#[test]` /
`#[tokio::test]` per file on 2026-10-27 (about 300 in the workspace, unit tests included),
plus Dart widget and unit tests (`app/test`, 38 files, `flutter test`).

Protocol and crypto (`crates/zafe-core/tests`)
- `zip2005_vectors` (1, regenerated and checked independently in Python), `keygen` (6:
  agreement on keys, rerandomized signature verifies as Orchard spend auth, equivocating
  relay, `sk` commitments), `keygen_async` (4), `session` (7), `one_tap` (5),
  `one_tap_retry` (2), `spend` (3: FROST spend of an Ironwood note, foreign note, wrong
  alpha), `verify` (9 negative PCZT cases), `reshare` (3, reference only), `backup` (5),
  `zcash_sign_crosscheck` (2, one ignored: against frost-tools `zcash-sign`).
- Coordination: `vault` (40: replay, versions, reservation, pools and partial groups,
  seat moves, repair state, unapproved-spend rules), `node_keygen` (7: members over an
  HTTP relay with the hosted limits), `cancel_setup` (2), `repair` (3), `relay_wait` (3),
  **`relay_rollback` (14: older log, forked history, wiped relay, restore from a member's
  copy, catch-up, never overwrite another history, restore in progress, mailbox still
  being set up, membership check, fork recovery, labels, anchored restore and recovery,
  spend alert from the copy)**, **`relay_move` (16: version gate, old relay alive or dead,
  the m threshold, writes before and after the switch, cancels, two moves, expired or
  replayed moves, stragglers)**, `relay_labels` (4), `relay_response_cap` (1),
  `log_cache` (3).
- Network: `tls` (5, one live), `tor_policy` (6, one live), `lightwalletd_timeout` (1).
- Live on regtest (Docker + `ths`): `regtest_e2e` (ignored), `app/rust/tests/bridge_e2e`
  (ignored; the app's payment flow through the bridge API), `scripts/m0-e2e.sh` (three CLI
  processes through the relay), `scripts/move-e2e.sh` (the same three move the vault
  between two relays).

Relay (`crates/zafe-relay/tests`): `relay` (16), `labels` (12), `quotas` (9 incl. stale
reseed), `expiry` (7), `persistence` (6), `wait` (6), `fcm` (2); unit tests in `limits`,
`wait`, `main`.

Protocol types (`crates/zafe-proto/tests`): `envelope` (6), `log` (4), `version` (5).

Bridge and app: `app/rust/tests` (`repair_bridge`, `vault_watch`, `payment_request`),
Dart `app/test` (sync failures, unlock gate, app lock, recipients CSV, invite and payment
links, relay moves and the relay picker, Tor setting, network config, ...).

Gaps we know of (please probe): no fuzzing of `Envelope`/`LogEntry`/`PCZT` decoders beyond
the negative tests above; no property tests for `VaultState::apply` (determinism is
tested by replaying fixed logs); `verify.rs` value-overflow path has no test (the PCZT
that triggers it can't be built through the public builders); no test that runs the relay
under load; iOS untested.

## 6. Pinned dependencies and upstream status

Exact pins are in `Cargo.toml` (spec §4.4): `reddsa =0.5.2` (`frost`), `frost-core` /
`frost-rerandomized` 3.0.0, and the librustzcash NU7 pre-release line: `orchard =0.16.0`,
`pczt =0.10.0-pre.1`, `zcash_primitives =0.31.0-pre.1`, `zcash_protocol =0.11.0-pre.0`,
`zcash_client_backend =0.25.0-pre.1`, `zcash_client_sqlite =0.23.0-pre.1` (mainnet's NU7
height 3,543,405 is set by Zafe, `wallet::MAINNET_NU7_HEIGHT`, because no release has it yet);
SQLCipher through `bundled-sqlcipher-vendored-openssl`, rustls + ring + webpki-roots
(1.0.10), arti 0.35 (Tor). Flutter 3.47.7 (`app/.fvmrc`),
`flutter_rust_bridge` 2.11.1 (checked 2026-10-27).

Upstream gates: U1 (ZIP 2005 derivation with our own `sk` agreement), U3 (redpallas moves
to the FROST repo, same serialization) and U5 (builder-chosen `alpha`) answered by ZF on
frost#1094 (2026-10-01). `upstream-asks.md` has the details; nothing upstream blocks the
audit.

## 7. How to build, run and attack it

```bash
cargo fmt --all && cargo clippy --workspace --all-targets
cargo test --workspace                 # ~300 tests; Halo 2 needs the optimized dev profile
ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors && python3 scripts/check_zip2005_vectors.py
cargo test -p zafe-core --test relay_rollback           # rollback and restore of the relay
cargo test -p zafe-core --test relay_move               # moving a vault between relays
cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture   # starts its own regtest (Docker + ths)
scripts/m0-e2e.sh                      # three `zafe` CLI members through a relay on regtest
scripts/move-e2e.sh                    # the same three move the vault between two relays
cargo test -p rust_lib_zafe --test bridge_e2e -- --ignored --nocapture
```

The headless member (`crates/zafe-cli`) is the easiest way to run a hostile participant:
it speaks the whole protocol from the command line against a local relay
(`target/debug/zafe-relay`), and its state is plain files (dev only). `zafe-relay` runs
with `ZAFE_RELAY_LIMITS=off ZAFE_RELAY_QUOTAS=off ZAFE_RELAY_EXPIRY=off` for experiments.

## 8. Findings so far (internal review, 2026-10-07)

Fixed during this review:
- `verify_pczt` summed note values with unchecked `u64` additions. A PCZT cannot become a
  valid transaction with out-of-range values (the proof and binding signature would fail),
  but a wrap could have made the fee equation lie on a screen. Now checked
  (`VerifyError::ValueOverflow`).
- Relay `reseed` (new) carries a timestamp, like the read routes, so a recorded request
  can't be replayed to hand a restored mailbox an outdated member list.
- One-tap assignment was all-or-nothing: one member without a pool (a replaced phone,
  a CLI member) silently turned one-tap off for everyone. Not a safety bug (the fallback
  is the interactive protocol) but a usability cliff; fixed with a version gate.

Observed, not changed (for the auditors to weigh):
- `Reseed` lets the relay store entries whose authors aren't current members (seats
  move). A member who restores a log can't forge entries (they must carry valid signatures
  and chain), but it can append real entries signed by former members. Clients verify
  every entry's author against the membership as of that entry, so such entries are
  rejected by everyone; the relay does not.
- The relay's `join` for a reseeded mailbox is impossible by construction (its token hash
  is zero), and `create`/`seal`/`remove`/`threshold` on a mailbox mid-restore are only
  reachable by its creator.
- `RelayClient::new` picks the log copy from a process-wide setting
  (`log_cache::configure`); the bridge and CLI configure it at startup (and the app in
  every isolate). A caller that forgets to configure gets no rollback protection and no
  error. Worth a look when reviewing `app/lib/main.dart` and
  `app/lib/src/notifications/vault_watch.dart`. *Since then the bridge fails closed: its
  `identity()` refuses to run without a configured copy. `RelayClient::new` itself still
  doesn't check.*

## 9. Adversarial review of b980c35 / 3163051 (2026-10-07, read-only, nothing run)

Fixed on branch `audit-fixes` (regression tests written, **not run**):
- **Log copy could shrink under concurrent writers (low).** `LogCache::write` checked the
  stored length and renamed in two steps; two bridge threads could leave the shorter chain,
  a transiently weaker rollback anchor. Now serialized per process (`WRITE` mutex); another
  process can still race, and the next `load_log` rewrites the longer chain. Test
  `concurrent_writers_never_leave_a_shorter_copy`.
- **Mainnet promotion provenance not pinned to `main` (low).** `gh attestation verify` only
  checked the signer workflow; `relay-deploy.yml`'s image job also ran on a manual dispatch
  from any branch (pushing `:main` and an attested image). The image job now needs
  `refs/heads/main`; the verify step adds `--source-ref refs/heads/main` and the OIDC issuer.
- **Soak check could pass on a bad record (low).** `at=null` read as 0 in bash arithmetic;
  now must be numeric. Litestream env values are single-quoted (a `$` in a secret broke Compose
  interpolation); unused `id-token: write` dropped.

Confirmed, not fixed at the time (most are fixed since: see §10 and the notes below):
- **App wiring is not on main.** *(Fixed since: the bridge configures the copy and fails
  closed without it.)* `log_cache::configure` / `init_log_cache`, the bridge error
  kinds and `restore_relay` live on `origin/audit-prep-app` (457fff7), not in b980c35. On
  main the app builds `RelayClient::new` with no copy, so there is no rollback protection
  in the app, while AGENTS.md describes it as shipped. Merge that branch before the audit
  commit is named. Never ship a path that skips `configure` silently (consider failing
  closed in `RelayClient::new` outside tests).
- **Reseed lets the restorer pick `members`, `threshold` and `enc_pk`s (medium, availability
  only; client check added, §10).** A single malicious member restoring a wiped relay can omit honest members (they
  get 403), add keys of its own, or set `threshold = 1` so the relay's `replace` route moves
  seats on the relay with one signature. Clients ignore all of it (membership comes from the
  replayed log; entries by unknown keys are rejected), so funds are unaffected, but honest
  members are locked out of that relay and must move to another one. Mitigation to build: a
  client check after a restore that `relay.members()` equals the replayed membership.
- **`catch_relay_up` cannot append entries by former seat keys (low).** The normal append
  route requires the author to be a current relay member; across a seat move a rolled-back
  relay with the old member table rejects them, and the new key is not a member there.
  Restoring such a vault needs the wipe path (reseed on an unknown mailbox), not catch-up.
- **A fork is a dead end for the losing device (low-medium, availability; fixed, §10).** After
  `RelayForked` nothing clears that device's copy (`LogCache::remove` has no caller), so a
  member on the minority branch of an equivocating relay is refused on every relay until
  reinstall or restore from backup. Add an explicit, confirmed "discard my copy" action.
- **Image pinning:** `litestream/litestream:0.3.13` and `caddy:2.11-alpine` are tags, not digests (fixed, §10).
- **`Broadcast` is not checked against the PCZT (pre-existing, low; fixed, §10).** Any member can log
  `Broadcast { txid }` with any txid for an Approved proposal; it sets status Broadcast
  (releasing the log's note reservation) and makes that txid count as "logged as sent" for
  the unapproved-spend alert. The txid of a v6 transaction is computable from the PCZT:
  compare at replay (gate with an event version).
- **Unapproved-spend grace** also shelters notes of cancelled/rejected proposals for 24
  blocks (code accepts any proposal, docs say "logged proposal"); narrow to live proposals
  (fixed, §10).

Reviewed, no issue found: one-tap partial groups (`assign_commitments` v6: cumulative
`taken` check matches the draw loop, every commitment drawn once, deterministic order,
old-version events keep the old rule, a v6-only log is skipped whole by v5 apps so pool state
cannot diverge into a double assignment; nonce safety still rests on delete-before-share);
`verify_pczt` checked arithmetic (all `u64` sums; fee and expiry use checked/guarded math);
`vault_spends` SQL (typed `[u8; 32]` reads error, not panic); `reseed` replay (idempotent by
`from`, timestamp window, signer-only continuation, quotas and capacity roll back with the
transaction); `load_log` anchor checks (index and hash of the last saved entry, 404 vs empty
page vs fork) and atomic cache writes.

## 10. Fixes for the section 9 items (2026-10-07, branch audit-fix2)

- **Broadcast txid.** `VAULT_EVENT` 7. A v6 txid commits to effecting data only (no spend-auth
  signatures, no proof), so it equals the shielded sighash that members already compute and sign;
  vaults hold no transparent inputs, so no other digest differs. `VaultState` stores it per
  proposal at replay (`expected_txid`); a `Broadcast` written with version 7 whose txid differs
  is ignored (`TxidMismatch`). Chosen over "one of several candidate txids" because the PCZT
  fixes exactly one. `regtest_e2e` asserts the real extracted transaction's txid equals it.
  A PCZT that does not parse skips the check (never signable). Versions 1-6 broadcasts are
  unchecked forever (old logs replay unchanged); every member must update before relying on it.
- **Unapproved-spend grace** now covers only approved or broadcast proposals, and only the
  transaction their PCZT fixes (`expected_txid`).
- **Reseed membership.** The blind relay cannot verify a restorer's members/threshold (the
  descriptor is encrypted), so clients do: `node::check_relay_membership` (new route
  `/v1/mailbox/info`, no `RELAY_API` bump) runs in `load_state` and before catching a relay up;
  `RelayMembership` -> bridge `RelayMembership` -> sync sheet copy. Test
  `a_relay_restored_with_other_members_or_threshold_is_refused` (threshold 1, extra key, member
  left out). Residual: a relay lock-out is still possible until members move to a fresh relay;
  a device without a relay route (older relay) checks members only.
- **Fork recovery.** `node::follow_relay` (bridge `follow_relay`, sync sheet "Follow the relay
  instead" behind a confirmation and unlock). Verifies the relay's whole log without the anchor,
  refuses when there is no fork, drops local-only entries. Residual: a device that follows a
  relay which equivocates to others follows that branch; the confirmation tells the user to
  check with the other members first.
- **Image pinning.** Litestream and Caddy are pinned by digest in `infra/relay/vps/compose.yml`.
- **Separate deploy access for testnet and mainnet (closes the section 9 "same SSH key" finding;
  files prepared 2026-10-07, active since the migration on 2026-10-11).** Each network has its
  own unix user and SSH key; the key is pinned
  to `sudo -n /usr/local/sbin/zafe-deploy <network>` (`restrict`, forced command, no shell, no
  `docker` group). The mainnet key exists only as a `relay-mainnet` environment secret (reviewer
  gate), the testnet job cannot read it and the server would not let the testnet key act on
  mainnet anyway. `zafe-deploy` (root-owned, `infra/relay/vps/zafe-deploy`) parses a fixed
  grammar, validates the image (`ghcr.io/zafe-cash/zafe-relay@sha256:<64 hex>`), the domain
  (pinned per network in `/etc/zafe-deploy.conf`) and secret file contents, installs files by
  downloading the commit from GitHub and refusing any commit not on `main` (CI sends no
  files), and only writes its network's secret files; the mainnet soak/digest check runs on
  the server. `deploy.sh` converges only its network's services; the relays use separate
  Docker networks. **Residual risk:** one host and one Docker daemon. `zafe-deploy` runs
  `deploy.sh` from `main` as root, so whoever can push to `main` (or a root-level escape from
  the testnet container) still reaches mainnet; the mainnet approval gate protects only
  against a leaked testnet key and mistaken pushes to the testnet job, not against a compromised
  `main`. A leaked testnet key can pick a `main` commit and any image digest of ours for
  testnet. Image provenance is verified in CI, not on the server. Mitigations to take: branch
  protection with required reviews on `main`, or a separate host for mainnet. `compare` against
  `main` uses the unauthenticated GitHub API (rate limit 60/h per IP): fine at deploy cadence.
  Not tested against a live server (no SSH from the build agent): the first migration run is
  the test; `bash -n` and a Compose config check only.

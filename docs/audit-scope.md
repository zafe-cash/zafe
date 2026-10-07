# Zafe: external audit scope

For the auditors, and for us to check that nothing else stands between the code and
mainnet. Written 2026-10-07 at the end of the "before mainnet" work; the commit to audit
is the one the audit engagement letter names (record it in `docs/tracker.md`). Product
spec: `spec.md` (protocol §5-§10, security model §13). Invariants and gotchas:
`AGENTS.md`. Open questions to Zcash Foundation: `upstream-asks.md`.

What Zafe is, in two sentences: a Safe-style shielded multisig for Zcash. A vault is an
Orchard-type (Ironwood pool) account whose spend authority is a **re-randomized FROST**
(t-of-n, RedPallas) key, with quantum-recoverable viewing keys per **ZIP 2005**; members'
phones coordinate through a **blind relay** that only sees ciphertext and public keys.

## 1. What to audit, in priority order

Lines are Rust, `wc -l` of the files named (2026-10-07). **P0 = loss or theft of funds if
wrong.**

| Pri | Area | Files | What can go wrong |
|---|---|---|---|
| P0 | Vault keys (ZIP 2005 `use_qsk = true`) | `zafe-core/src/keys.rs` (302), `tests/zip2005_vectors.rs`, `scripts/check_zip2005_vectors.py` | wrong derivation = unrecoverable or unusable vault; vectors are also cross-checked in Python and against frost-tools |
| P0 | DKG, `sk` agreement | `keygen.rs` (410), `node::run_keygen` | a member (or the relay) biasing or learning `sk`/the group key; echo, commitments to `sk` contributions, HPKE sealing |
| P0 | Independent verification of a PCZT | `verify.rs` (290), `tx.rs` (151) | a member approving or signing a transaction that pays anyone but the proposed payees; fee, change, spends, sighash |
| P0 | FROST signing, randomizer, nonces | `signing.rs`, `session.rs` (585), `nonce_store.rs` (147), `node::{approve, sign_own_shares, send_ready, finalize}` | nonce reuse (key leak), signing a different message, accepting a bad share, `alpha` leaking |
| P0 | One-tap signing and replay determinism | `vault.rs` (1050): `assign_commitments`, `apply`, `ProposalState`; `node::{top_up_pool, forget_closed}` | members computing different states; a commitment assigned twice; shares for a group the member isn't in |
| P0 | Note reservation, sweeps, expiry | `vault.rs` (`NotesInUse`), `node::{reserve_notes, note_holds, invalidate}`, `wallet.rs` | double-spending a note into two payable proposals; a cancelled but fully signed transaction staying sendable |
| P1 | Relay-facing logic that guards against a hostile relay | `node.rs` (`load_log`, `catch_up`, `reseed_relay`), `log_cache.rs`, `zafe-proto/src/{log,envelope,identity}.rs` | a relay forging, forking, rolling back, reordering or replaying; signature, AEAD and hash-chain checks |
| P1 | Membership change and share repair | `repair.rs` (594), `vault.rs` seat moves, relay `replace_member` | taking over another member's seat; leaking or corrupting a share during repair (RTS) |
| P1 | Backups | `backup.rs` (411) | weak KDF, nonces in a backup (never allowed), restoring an identity that isn't a member |
| P1 | Wallet and secrets at rest | `wallet.rs` (SQLCipher open, key handling), app `core/storage`, `core/security` | key in the wrong place, DB opened unencrypted, unlock gate bypass |
| P1 | Relay server | `zafe-relay/src/lib.rs` (1370), `limits.rs`, `quota.rs`, `wait.rs` | auth of every route, quota/rate bypass, SQL, a member reading or writing another vault's mailbox, `reseed` |
| P2 | App | `app/lib`, `app/rust/src/api` | UI states that show unverified data; bridge errors; deep links (`zcash:`, invites) |
| P2 | Tor, TLS, price | `tor.rs`, `net.rs`, `relay_client.rs`, `price.rs` | traffic leaving outside Tor when Tor is on; certificate handling |
| P2 | Infra | `infra/relay/**`, `.github/workflows/relay-*.yml` | deploy path; secrets handling; the digest promoted to mainnet |
| P2 | Unapproved-spend alert | `spend_watch.rs`, `wallet.rs::vault_spends` | false negatives (a theft the alert misses) |

Not in scope: Flutter and plugin internals, `zcash_client_backend`/librustzcash,
`frost-core`/`reddsa`/`orchard` (we pin them, see §6; we ask for a review of **how** we use
them), the landing site (`infra/site`), brand assets, scripts that only build art.

## 2. Threat model

Assets: vault funds; privacy of balances, payees and amounts (viewing keys); availability
of the approval flow (nothing may brick funds, nothing may stop members from spending).

| Adversary | Can | Must not |
|---|---|---|
| Relay operator or a network attacker | see metadata (who, when, sizes, IPs, mailbox ids, push tokens); drop, delay, reorder, replay messages; serve stale, forked or shorter logs; refuse service | read content; forge a member's message, vote or log entry; make a member sign anything the member has not verified; make a device accept a log shorter than the one it saw (device copy, spec §6.3); learn `sk`/shares |
| One or fewer than t malicious members | read everything (they hold `sk`); propose anything; refuse to approve; try to confuse others with odd logs or messages | spend; make another member sign a transaction that fails that member's own check; reuse another member's nonce |
| The proposer or the leader (a member) | choose the PCZT and `alpha`; choose who completes a signing round | change payees, amounts, memos, fee, change destination, or the sighash another member signs; learn more than the signature shares |
| t or more colluding or compromised members | spend, bypass rules | (inherent: spec §13.1; detected after the fact by the unapproved-spend alert) |
| A compromised phone | use that member's share while unlocked or whenever the app can sign without a prompt (background auto-send finishes only approvals the owner already gave) | read the share from backups (`allowBackup=false`, nonces never in backups); sign without the unlock gate when "Require unlock to approve" is on |
| A removed member who kept an old share | sign at the old threshold (spec §10.4.4; fixable only by migrating) | (detected by the unapproved-spend alert) |
| A quantum adversary (future) | break discrete log: with one member's `qsk` recover spend authority to a vault (spec §2.3, ZIP 2005) | (accepted; not fixable inside ZIP 2005) |
| Malware or another app on the phone | read app-private storage on a rooted device | (out of scope beyond OS sandboxing, SQLCipher, secure storage) |

Explicit trust assumptions: members compare the safety number out of band during setup;
OS secure storage and RNG work; upstream crypto crates are correct (§6); GitHub Actions
and the Docker host are trusted to deploy the relay (the relay is blind, so this is a
privacy and availability risk, not custody).

## 3. Protocol invariants (breaking one is a funds or security bug)

The full list is in `AGENTS.md` "Protocol invariants"; this is the audit checklist, with
the code that enforces each and the test that should catch a regression.

1. **Vault keys.** `ak` from the DKG; `nk`, `qsk`, `qk`, `rivk_ext` from the agreed `sk`
   exactly as ZIP 2005 § 4.2.3 with `use_qsk = true`. Never frost-tools'
   `from_sk_ak_incompatible_with_quantum_recoverability`. `keys.rs`; `zip2005_vectors`,
   `keygen::members_agree_on_vault_keys`.
2. **DKG integrity.** Safety number confirmed first; round-1 echo hashes equal across
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
   (`VaultState::apply_versioned`): version 6 assigns one-tap commitments per fully
   covered group, versions 1-5 keep all-or-nothing. `tests/vault.rs` (31 tests).
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
    `RelayLostVault`). `node::load_log`, `log_cache.rs`; `tests/relay_rollback.rs`.
12. **Versioned formats.** Every persisted or wire format has a tag; decoders accept only
    what they know and fail with `UnsupportedVersion` (never garbage); version bumps and
    their gates are recorded in `AGENTS.md`.
13. **Wallet DB encrypted** with a per-vault raw key held in secure storage; the open path
    refuses to run if SQLCipher isn't linked; a wrong key resyncs, never opens plain.
14. **Backups never contain nonces or the wallet key**; the KDF floor is 64 MiB / 3 passes.
15. **Relay authentication.** Every state-changing or reading route verifies a signature,
    charges the key's rate limit only after it verifies, and checks membership; reads and
    `reseed` carry a timestamp (±300 s). Quotas keep running byte counters updated in the
    same transaction as the write.

## 4. Known deviations, accepted risks and open questions

| Item | Status |
|---|---|
| **ZIP 312 randomizer.** ZIP 312 (zips#895 draft) says the Coordinator derives the randomizer after round 1 from fresh bytes and the commitment list. Zafe uses the builder's `alpha` (fixed before round 1) because one-tap commitments exist before any proposal and several signer groups share one `alpha` per spend. Security: Re-Randomized FROST (ePrint 2024/436) proves unforgeability for adversarially chosen randomizers; `alpha` is public to signers. Confirmed acceptable by ZF (conradoplg, frost#1094, 2026-10-01) and Daira-Emma Hopwood (Discord); requirement: `alpha` links `rk` to `ak`, so it must stay within the vault. Spec §9.5.1; asked as U5. | accepted |
| **Deprecated `frost_rerandomized::sign`** used in one wrapper (`signing.rs`) because it is the only external-randomizer API; ZF says an external-randomizer API will stay (U2). | accepted |
| **`sk` agreement is our own scheme**, not COCKTAIL-DKG (frost#1033, not production-ready, no Pallas). ZF: "if you have your own system that will also work" (U1). Align when COCKTAIL-DKG ships. | open upstream |
| **Post-quantum:** every member holds `qsk`, so one member's `qsk` + a discrete-log break is enough to recover spend authority; ZIP 2005 says so. Not fixable inside ZIP 2005. | accepted, told to users (`/vault-protection`) |
| **Messaging crypto is the previous generation** (`ed25519-dalek` 2, `hpke` 0.12, `chacha20poly1305` 0.10): the newer set needs stable `sha2` 0.11, which conflicts with a pin in `zcash_client_backend`. | accepted until pins move |
| **Ceremony and repair messages are sealed to long-term identity keys** (no forward secrecy): someone who recorded them and later steals an identity key learns what those messages carried. One-time ceremony keys are planned before refresh/reshare ships (spec §10.4.4). | accepted for v1 |
| **First load trusts the relay once.** A device with no copy of a vault's log (first load after join, restore or reinstall) can't detect a relay that serves it a shortened or forked log. After the first load it can. | accepted; documented in spec §6.3 |
| **A relay restored from an old database can lose in-flight messages and push tokens** until members ask again. Funds and the vault log are unaffected (members restore the log). | accepted |
| **A hostile relay can always deny service**, and can claim an unknown mailbox id first after a wipe (it needs the 16-byte id); members then can't restore there. They move to another relay and restore from a device copy. | accepted |
| **Rules, address book, rotation, resharing, migration** are not implemented (spec §10-§11, M3): a vault can't change members or `t` except by seat moves; a removed device keeps its share. Mainnet beta ships without them. | out of scope for v1; the unapproved-spend alert covers old shares |
| **Unapproved-spend alert has a grace window** (24 blocks) in which a spend that spends a logged proposal's notes isn't flagged; a thief who only re-signs an existing proposal's notes to another address is flagged after the window, not at once. | accepted |
| **iOS has never been built.** Backup exclusion, Notification Service Extension and background behaviour are untested. Android only for the beta. | out of scope |
| **Android background checks sign nothing new**: they answer interactive requests and finish auto-sends the owner already approved, without an unlock prompt, by design. | accepted |
| **One relay per vault.** Moving to another relay is a manual restore from a member's copy. | by design |
| **Beta cap is advisory in the app** (money can't be refused on receive); the relay caps the number of vaults. | by design |

## 5. Test inventory

Run everything the CI runs (`AGENTS.md` "Commands"). Counts are `#[test]` /
`#[tokio::test]` per file, 2026-10-07, plus Dart widget and unit tests (`app/test`, 22+
files, `flutter test`).

Protocol and crypto (`crates/zafe-core/tests`)
- `zip2005_vectors` (1, regenerated and checked independently in Python), `keygen` (6:
  agreement on keys, rerandomized signature verifies as Orchard spend auth, equivocating
  relay, `sk` commitments), `session` (7), `one_tap` (5), `spend` (3: FROST spend of an
  Ironwood note, foreign note, wrong alpha), `verify` (9 negative PCZT cases), `reshare`
  (3, reference only), `backup` (5), `zcash_sign_crosscheck` (1 + 1 ignored: against
  frost-tools `zcash-sign`).
- Coordination: `vault` (31: replay, versions, reservation, pools and partial groups,
  seat moves, repair state, unapproved-spend rules), `node_keygen` (4: three members over
  an HTTP relay with the hosted limits), `repair` (2), `relay_wait` (3), **`relay_rollback`
  (7: older log, forked history, wiped relay, restore from a member's copy, catch-up,
  never overwrite another history, restore in progress, mailbox still being set up)**,
  `log_cache` (2).
- Network: `tls` (5, one live), `tor_policy` (5, one live), `lightwalletd_timeout` (1).
- Live on regtest (Docker + `ths`): `regtest_e2e` (ignored), `app/rust/tests/bridge_e2e`
  (ignored; the app's payment flow through the bridge API), `scripts/m0-e2e.sh` (three CLI
  processes through the relay).

Relay (`crates/zafe-relay/tests`): `relay` (11), `quotas` (8 incl. capacity and stale
reseed), `persistence` (5), `wait` (6), `fcm` (2); unit tests in `limits`, `wait`, `main`.

Protocol types (`crates/zafe-proto/tests`): `envelope` (7), `log` (4), `version` (5).

Bridge and app: `app/rust/tests` (`repair_bridge`, `vault_watch`, `payment_request`),
Dart `app/test` (sync failures, unlock gate, app lock, recipients CSV, invite and payment
links, privacy mode, beta cap, network config, ...).

Gaps we know of (please probe): no fuzzing of `Envelope`/`LogEntry`/`PCZT` decoders beyond
the negative tests above; no property tests for `VaultState::apply` (determinism is
tested by replaying fixed logs); `verify.rs` value-overflow path has no test (the PCZT
that triggers it can't be built through the public builders); no test that runs the relay
under load; iOS untested.

## 6. Pinned dependencies and upstream status

Exact pins are in `Cargo.toml` (spec §4.4): `reddsa =0.5.2` (`frost`), `frost-core` /
`frost-rerandomized` 3.0.0, `orchard =0.15.5`, `pczt =0.9.3`, `zcash_client_backend
=0.24.0`, `zcash_client_sqlite =0.22.0`, SQLCipher through `bundled-sqlcipher-vendored-
openssl`, rustls + ring + webpki-roots, arti 0.35 (Tor). Flutter 3.41.6,
`flutter_rust_bridge` 2.11.1.

Upstream gates: U1 (ZIP 2005 derivation with our own `sk` agreement), U3 (redpallas moves
to the FROST repo, same serialization) and U5 (builder-chosen `alpha`) answered by ZF on
frost#1094 (2026-10-01). `upstream-asks.md` has the rest.

## 7. How to build, run and attack it

```bash
cargo fmt --all && cargo clippy --workspace --all-targets
cargo test --workspace                 # ~100 tests; Halo 2 needs the optimized dev profile
ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors && python3 scripts/check_zip2005_vectors.py
cargo test -p zafe-core --test relay_rollback           # rollback and restore of the relay
infra/regtest/up.sh && cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture
scripts/m0-e2e.sh                      # three `zafe` CLI members through a relay on regtest
cargo test -p rust_lib_zafe --test bridge_e2e -- --ignored --nocapture
```

The headless member (`crates/zafe-cli`) is the easiest way to run a hostile participant:
it speaks the whole protocol from the command line against a local relay
(`target/debug/zafe-relay`), and its state is plain files (dev only). `zafe-relay` runs
with `ZAFE_RELAY_LIMITS=off ZAFE_RELAY_QUOTAS=off` for experiments.

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
  `app/lib/src/notifications/vault_watch.dart`.

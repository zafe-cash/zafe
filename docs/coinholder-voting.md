# Coinholder voting for vaults

Research 2026-11-19, revised 2026-11-20 after a source review (see "Review log"). Nothing
is built yet. Upstream moves fast: check versions before building on this.

Sources read: `zcash_voting` 5.4.0, `voting-circuits` 0.15.0, `voting-crypto-deps` 0.5.0
(crates.io), `valargroup/vote-sdk` at d51f67b (2026-11-18), the ZODL wallet SDK, and the
live rounds at `prod.vote-chain-primary.valargroup.org/shielded-vote/v1/rounds`.
Background: [The Coinholder Voting Chain](https://forum.zcashcommunity.com/t/the-coinholder-voting-chain/56925).

## How the protocol works

Valar Group's shielded voting runs on a separate Cosmos chain. A 2-of-5 coordinator
multisig creates rounds; the validators hold the election key (DKG, threshold decryption).
Vizor, ZODL, Zkool and Keystone support it.

1. **Delegation (ZKP1).** The holder proves it held unspent **Ironwood** notes at the
   round's snapshot (up to 5 notes per bundle; 1 ballot = 0.125 ZEC). The weight becomes a
   Vote Authority Note (VAN) owned by a **hotkey** address.
   - Funds authorise it with **one RedPallas SpendAuth signature per bundle**, under
     `rk = ak + [alpha]G`, over the sighash of a never-broadcast "TX1". The vote chain
     rebuilds that sighash and checks the signature (`vote-sdk x/vote/ante/validate.go:248-271`).
   - The signed dummy note's `rho = Poseidon(cmx_1..5, van_comm, round)` (delegation
     README, condition 3). `van_comm` hashes the hotkey address, the ballot count, the
     round and a blinding (condition 7). **So the signature fixes the notes, the weight,
     the round and which hotkey gets it. It does not fix the answers.**
2. **Vote (ZKP2).** The hotkey spends the VAN once per proposal (a 50-bit authority mask, so
   one vote per proposal and no changing it), casting an encrypted ballot split into
   shares. The vote is signed under `r_vpk = vsk·G + [alpha_v]G` with the hotkey's own key.
3. **Shares and tally.** Submission servers reveal the shares; validators decrypt only the
   totals.

The crate also has an **atomic path**: `prepare_delegate_and_vote_batch` takes an outside
delegation signature plus up to 50 votes and submits them in one message
(`src/delegate_and_vote_batch/preparation.rs:11-37`, `MAX_VOTE_BATCH_ACTIONS = 50`,
`src/vote.rs:34`). The chain checks the delegation in it exactly as a standalone one
(`validate.go:185-221`): the delegation isn't bound to the votes beside it.

Spec: the draft ZIP [zcash/zips#1200](https://github.com/zcash/zips/pull/1200), from
August, is older than the code: it still says Orchard, BIP 39 hotkeys and a sighash that
isn't rebuilt. Trust the crate and the circuits. Service discovery is
[zcash/zips#1244](https://github.com/zcash/zips/pull/1244).

### Rounds so far

| Round | Open | Proposals |
|---|---|---|
| NU7 Scope | 2026-08-24 → 09-14 (21 days) | NU7 scope |
| Coinholder Retroactive Grants Q3 | 2026-09-17 → 09-29 (12 days) | 38 grants, $3,050 to $1,950,000 each |
| NU7.1 Poll | 2026-11-16 → 12-01 02:59 UTC (status `ACTIVE`) | 5 (ZCG wind-down, dev fund, ZAP, 20 s blocks) |

- The grants round decides real money, which is what treasuries and DAOs care about. Its
  "Q3" title suggests it will recur.
- The snapshot is fixed when the round is created: the round carries the tree roots at that
  height (`x/vote/keeper/msg_server.go:97-107`), so only notes the vault held then count.
- Zafe can't be ready for the NU7.1 round.

## How ZODL does it

- `zodl-inc/zcash-android-wallet-sdk` `backend-lib/Cargo.toml` uses
  `zcash_voting = { version = "=5.3.0", default-features = false, features = ["lrz"] }`.
  It runs on **stable** librustzcash: `zcash_client_backend =0.25.0`, `pczt =0.10.0`,
  `orchard 0.16`.
- The crate's default feature, `zakura`, pulls in Valar's fork stack. ZODL says it would add
  about 4.2 MB of duplicate crypto. `lrz` resolves to the same versions as the rest of the
  wallet.
- Keystone delegation goes through `keystoneSigningRequests` / `storeKeystoneSignatures`
  (iOS `VotingCryptoClientInterface.swift`). This is the hook a vault uses too.
- The hotkey is a random 64-byte secret kept per wallet (`StoredVotingHotkey`).
- The config is a hash-pinned static document that points to a signed dynamic config
  (`docs/voting-service-discovery.md` in zodl-ios). With Tor on, a poll's traffic all goes
  over Tor.
- Vizor (`chainapsis/vizor-wallet` `rust/Cargo.toml`) takes the other route:
  `zcash_voting =5.4.0` with default features, on Valar's `zakura-*` forks (`orchard`
  is `zakura-orchard`). Since Zafe is on upstream librustzcash, ZODL's `lrz` route is ours.

## What a vault can and can't do

- **It can delegate with t of n.** Delegation needs only `ak`, `nk`, both `rivk`s and the
  note witnesses at the snapshot, which every member has. The spend authority is the
  FROST signature, built like a payment's, just as a Keystone signs in ZODL.
- **It can't cast a vote with its own key.** ZKP2 takes the hotkey's spending key as a
  scalar `vsk` and computes `[vsk]·SpendAuthG` in the circuit (vote proof README,
  conditions 3-4; `src/vote_proof/circuit.rs`, `spend_auth_g_mul`). Nobody knows a FROST
  vault's spending scalar. This is a **circuit limit**: fixing it needs a new circuit, a new
  verifying key on the vote chain and a chain upgrade.
- **So one key casts every vote.** The vault chooses which hotkey gets its weight (t of n).
  Whoever holds that hotkey, and the delegation signature, picks the answers.
- **Front-running is possible.** Messages go to a node's `/delegate-and-cast-vote-batch`
  endpoint and then through the CometBFT mempool, which every peer of the public vote chain
  sees. Anyone who knows the hotkey's `vsk` can copy the delegation out of a pending
  message and submit it with other answers first.

## Options for who holds the hotkey

### 1. A hotkey for each ballot, held by the leader and revealed afterwards (works today)

The leader derives a fresh hotkey from its own identity seed, the vault id, the round and
the attempt, and passes the 64 bytes to `VotingHotkey::from_stored_secret` (`src/hotkey.rs`).
Being derived, it survives a crash. It shows members only the hotkey **address** and the
VAN blinding, which they need to check TX1. Once the batch is on the vote chain, the leader
logs the hotkey secret, sealed to the members.

- **What it gives:**
  - Until the vote lands, nobody but the leader knows `vsk`, so no other member or old
    phone can front-run it.
  - After the reveal, every member checks the vote. VAN blinding (`src/van_blinding.rs`)
    and share randomness (`src/zkp2.rs:89-93`) come from the hotkey secret, so a member
    can recompute each expected new VAN and vote commitment. It then finds them among the
    vote chain's `CommitmentLeaves` (`proto/svote/v1/query.proto:30`).
  - A leader that never reveals is itself a signal.
- **What it costs:**
  - The leader can still cast other answers. Members find out afterwards, but a vote
    can't be undone. Option 3 is the only fix.
  - A leader that goes quiet after collecting shares keeps signatures it can use until
    the round closes. Starting over with a new leader works, and the first batch to land
    wins. This is the voting form of "a signed cancelled transaction can still be mined",
    and there is no sweep to stop it.
- **Rules that follow:**
  - **Cast every active proposal in the same atomic message as the delegation** (Abstain
    where the vault has no view), so the revealed hotkey has no authority left.
  - **Delegation signing is interactive only.** One-tap shares sit in the log, so any
    member could aggregate them.

### 1b. One hotkey shared by every member (derived from the vault secret)

- **What it gives:** any member can cast, and anyone can check.
- **What it costs:** every member and every old phone knows `vsk` before inclusion, so any
  of them can front-run the leader's message from the mempool.
- **Rejected:** option 1 gets the same audit without that window.

### 2. A delegate's own hotkey (a member or an outsider)

The vault t-approves "round R, all ballots → this hotkey address" and hands over a
`DelegationCapabilityV1` (`src/delegation_capability.rs`). This is ZODL's two-device flow,
and it suits a DAO that delegates to someone.

- **What it costs:**
  - The capability carries no keys, `alpha` or note data. It does carry the ballot count
    (the vault's balance at the snapshot, to 0.125 ZEC), the VAN blinding and the
    delegation transaction hash, so the delegate learns the vault's size.
  - The vault can't check how the delegate voted.
- **Verdict:** a variant of option 1's flow, with an outside target address and no answers
  to approve. Build it only if someone asks.

### 3. The vault's own key casts (t of n for the answers too)

The VAN goes to the vault's own address. The draft ZIP already allows a hotkey that reuses
the holder's keys ("Why VAN Nullifier Domain Separation"). Each vote is signed with FROST.

- **What it gives:**
  - The answers need t signatures, so there is nothing to front-run and no leader to
    trust.
  - No reveal or audit step.
  - The delegation and the vote batch are signed in one session: members approve the
    ballot once and sign once.
- **The circuit change is small.**
  - In ZKP2, `vsk` appears only in conditions 3-4 (`src/vote_proof/circuit.rs:912-923`,
    used again at `:957`). There it computes `ak = [vsk]·SpendAuthG`.
  - Witnessing `ak_P` as a `NonIdentityPoint` instead is exactly what ZKP1 does
    (`src/delegation/circuit.rs:776-790`) and what Orchard's action circuit does. The
    circuit even drops a fixed-base multiplication.
  - Security stays Orchard's: the authority is the vote signature under `r_vpk`, checked
    out of circuit. The circuit's own comment above condition 4 says so.
  - Software hotkeys keep working: they compute `ak` outside the circuit.
- **The chain change is routine for Valar.** It needs a new vote-proof verifying key, and
  Valar has shipped several:
  - v0.9 moved to orchard 0.15;
  - v0.10 brought K=11;
  - v0.12 brought 50 proposals (`voting-circuits` CHANGELOG).

  Each needed validators to deploy the new key between rounds (`vote-sdk`
  `docs/runbooks/software-upgrades.md`). `MsgCastVote` already carries `r_vpk` and the
  vote signature, so the wire format stays the same.
- **Crate changes in `zcash_voting`:**
  - a `VoteSigner` variant for an external signer (the enum is `#[non_exhaustive]`), which
    hands out signing requests and takes signatures back, as the Keystone delegation path
    does;
  - ZKP2 built from an FVK rather than a hotkey seed (`src/zkp2.rs:45`, `:229`);
  - a caller-chosen `alpha_v` (today `OsRng`, `src/zkp2.rs:272`), because FROST needs it
    before round 1, as for payments;
  - VAN blinding and share randomness from a secret the caller gives
    (`src/van_blinding.rs:30`, `src/vote_commitment.rs:129`); for us, derived from the
    vault secret.
- **Every other client** (ZODL, Vizor, Zkool) has to update the crate when the key
  changes. Valar already coordinates that for each key change.
- **Our side:**
  - FROST-sign the vote batch digest (`SVOTE_DELEGATE_AND_CAST_VOTE_BATCH_SIGHASH_V1`,
    `src/delegate_and_vote_batch/authorization.rs`), which every member rebuilds from the
    logged ballot before signing;
  - one signature per vote, or one per batch if every vote in a batch shares `alpha_v`
    (to check: the chain doesn't seem to reject a repeated `r_vpk`).
- **Status:** a search of `valargroup/voting-circuits`, `zcash_voting` and `vote-sdk` for
  multisig, FROST, external signer and `vsk` found no issues or PRs (2026-11-20).
- **Prototype:** [zafe-cash/voting-circuits `threshold-vote-signer`](https://github.com/zafe-cash/voting-circuits/tree/threshold-vote-signer)
  (9f07e9b, on 0.15.0). Proposed upstream in
  [valargroup/voting-circuits#145](https://github.com/valargroup/voting-circuits/issues/145) (2026-11-20).
  - **Change:** ZKP2 witnesses `vsk_ak`. It adds `build_vote_proof_from_fvk` and
    `VotePrfKey`; the `SpendingKey` path derives byte-identical shares.
  - **Size:** the circuit diff is about 60 lines. It saves one fixed column; the
    2,015-row high-water mark and K=11 stay. Only the vote-proof VK changes.
  - **Tests:** a 2-of-3 FROST group proves and signs a vote with no spending key,
    checked by both vote-chain verifiers (zakura-reddsa 5.4, reddsa 0.6). There are
    negative tests for a signer key that doesn't own the VAN, an identity `ak` and the
    accepted `-ak` case. All suites pass on both backends.
  - **Review:** an independent review found no soundness or circuit defect. One
    consequence to state upstream: the proof no longer shows knowledge of `ask`, so the
    vote-chain signature check is the only authority check. Every cast path already
    requires it (`validate.go` 199, 342, 397).
  - **Open:**
    - The test signs a stand-in digest, not the cast-vote sighash.
    - The FROST dev-dependencies add about 22 lockfile packages; Valar may want the
      test elsewhere.
    - vote-sdk pins `voting-circuits =0.12.0`, so adoption needs a version bump and a
      coordinated chain and wallet release.
- **Verdict:** the target design. It depends on Valar agreeing, a review of the circuit
  diff and a chain upgrade between rounds. Option 1 is the fallback if that stalls.

### 4. Split the weight across members' hotkeys

- Weight follows notes, not seats: each bundle of up to 5 notes goes to one VAN, so the
  split would be uneven.
- It would also show each member's share.
- **Rejected.**

### 5. Don't vote

The default. A custodial vault, or one whose members can't agree, ignores rounds. Voting
is opt-in for each round.

## Flow (option 1 today; option 3 replaces steps 5-6 with FROST-signed votes)

1. **Find rounds.** Use ZIP 1244 discovery with the pinned config hash. Show a vote card
   only when the vault held at least 0.125 ZEC in Ironwood at the snapshot.
2. **Propose a ballot.** A member gives one answer for every active proposal (Abstain
   allowed). The ballot is a new vault event, approved by t members like a payment, with
   interactive commitments. When two ballots both reach t, the first one to land wins and
   the other fails on the governance nullifiers.
3. **Build.** The leader:
   - derives the ballot hotkey;
   - plans the bundles (`note_bundling.rs`; external and change notes may share a bundle,
     since the circuit has `ivk` and `ivk_internal` with a per-note scope flag);
   - builds one TX1 per bundle with a fresh `alpha`.
4. **Sign.** Each member checks every TX1 (next section) and signs. One signing session
   covers all bundles.
5. **Prove and submit.** The leader runs ZKP1 for each bundle, then ZKP2 for each
   (bundle, proposal). It submits one `DelegateAndCastVoteBatch` per bundle, in ascending
   proposal order, and logs the submission with its share policy (`single_share` depends
   on timing, `src/vote_work/cast_vote.rs:53`, and changes the commitment).
6. **Reveal and check.** The leader logs the hotkey secret. Every member recomputes the
   expected leaves for the logged ballot and finds them on the vote chain. A mismatch, or no
   reveal by the end of the round, raises an alert ("a vote the vault didn't approve"), as
   `spend_watch` does for spends.

## What a member checks before signing a delegation

The delegation is a new message signed with the vault key, so it gets its own verifier
next to `verify::verify_pczt`. It never signs a sighash handed over by someone else.

- Rebuild TX1 and **compute the sighash locally**. TX1 must be exactly one Ironwood
  action: NU6.3 branch id, expiry 0, value balance +1 zat, no other bundles
  (`src/tx1.rs:1`).
- The spend is the synthetic 1-zat note (condition 1), with `rho` recomputed from:
  - the bundle's `cmx`s, each a vault note this member's wallet held unspent at the
    snapshot;
  - `van_comm`, from the leader's hotkey address, the ballot count for those notes, and
    the given blinding;
  - the round id, authenticated against the pinned voting config.
- The logged ballot has t approvals and covers every active proposal.

`verify_pczt` already rejects TX1 (branch id, expiry), so a delegation can't pass as a
payment. A TX1 can't be mined either: its only spend is a synthetic note that isn't in the
tree, and its branch id is no longer current. **`alpha` stays inside the vault**, as for
payments: only the vote chain sees `rk`.

## Blockers and other work

- **Version pins (first step, worth doing on its own).**
  - Zafe pins the NU7 *pre-releases* (`zcash_protocol =0.11.0-pre.0`,
    `zcash_client_backend =0.25.0-pre.1`, `zcash_client_sqlite =0.23.0-pre.1`,
    `pczt =0.10.0-pre.1`).
  - The stable releases are out (`zcash_protocol 0.11.0`, `zcash_primitives 0.31.1`,
    `zcash_keys 0.17.0`, `zcash_client_backend 0.25.0`, `zcash_client_sqlite 0.23.0`,
    `pczt 0.10.0`).
  - A probe crate depending on `zafe-core` and `zcash_voting =5.4.0` with `lrz` fails to
    resolve on `zcash_protocol`.
- **Voting DB.** `VotingDb` opens its own plain SQLite file (`src/storage/mod.rs:247-262`).
  Its `bundles` table stores `alpha` (`src/storage/migrations/001_init.sql:28`), so it can't
  sit unencrypted next to our SQLCipher wallet. Options: run it in memory and keep our own
  sealed state, or ask upstream for a keyed connection.
- **Vault log.** New events: a ballot proposal and its approvals, the submission record,
  and the hotkey reveal. This needs a `VAULT_EVENT` bump, gated on every member. Only logged
  approvals release shares (rule 5).
- **Network.**
  - Fetch the lightwalletd tree state through `wallet::connect` and pass it with
    `DelegationLwdInputs::from_anchor_tree_state` (`src/delegate.rs:508`), not the crate's
    own channel.
  - The vote chain, PIR and helper HTTP go through `RouteHttp` (`src/http_transport.rs:307`)
    behind our Tor choke point. The crate's observability stays off.
- **Witnesses.** These come from the lightwalletd frontier plus
  `generate_ironwood_witnesses_at_historical_height` (`src/witness.rs:200`), not our 100
  checkpoints. A vault created after the snapshot can't vote that round.
- **Windows.** Rounds have stayed open 12-21 days. t approvals and the cast must land
  inside that window, so the ballot needs a push and a "closes in" line.

## Still to verify (regtest spike)

- **ZIP 2005 keys.** ZKP1 and ZKP2 run on a `use_qsk` vault FVK. The source says yes: with
  `lrz`, `rivk` comes from `FullViewingKey::rivk(scope)` (`voting-crypto-deps`
  `src/lib.rs:373-385`), the same FVK the wallet uses. Prove it end to end.
- **Notes spent since the snapshot** are selected and witnessed by the crate from our
  wallet DB.
- **Shares are deterministic.** Given the hotkey secret and the share policy, a member
  rebuilds byte-identical leaves. Option 1's check rests on this, and so far it comes only
  from the doc comment at `src/zkp2.rs:89-93`.
- **Proving time on our phones.** ZKP1 per bundle, and ZKP2 per (bundle, proposal): 38
  proposals with 2 bundles is 76 vote proofs.
- **Vote-chain regtest.** Either `vote-sdk` locally or Valar's testnet chain.

## Review log

**Round 1** (claim, evidence, verdict):

| Earlier claim | Evidence | Verdict |
|---|---|---|
| "ZKP2 only needs `ak`, `nk`, `rivk`: an API limit, not a circuit limit" | ZKP2 condition 3 computes `[vsk]·SpendAuthG` in the circuit | **Wrong.** Casting with the vault key needs a new circuit (option 3) |
| "B: t approve the answers, FROST signs the vote" | The FROST signature binds hotkey, weight and round (`rho`); the batch digest is signed under `r_vpk` only | **Wrong.** No mechanism today; option 1 detects, option 3 would prevent |
| "No round was open" | NU7.1 Poll is `SESSION_STATUS_ACTIVE` until 2026-12-01 | **Wrong** |
| "External and internal notes need separate bundles" | `ivk_internal` and per-note `is_internal` in ZKP1; `zkp1.rs` carries scope per note | **Wrong**, dropped |
| "ZIP 2005 FVKs: unverified" | `rivk` from the FVK under `lrz` | Source says yes; the spike still proves it |
| "Delegation is t of n" | The chain verifies the RedPallas signature over the rebuilt TX1 sighash | **Holds** |
| "Hand the delegate a capability" | The capability carries ballots, VAN blinding and tx hash; no keys or `alpha` | Holds, but the delegate learns the vault's size |

**Round 2**, on round 1's first recommendation (one hotkey shared by every member):

| Claim | Evidence | Verdict |
|---|---|---|
| "Casting atomically closes the window; only the leader can deviate" | The batch's delegation is checked alone (`validate.go:185-221`) and pending messages sit in the public mempool | **Wrong**: any holder of `vsk` can front-run. Moved to 1b, rejected; option 1 keeps `vsk` with the leader until inclusion |
| "Members can check the vote" | Leaves are queryable (`CommitmentLeaves`); VAN blinding and shares derive from the hotkey secret; `single_share` changes the commitment | Holds once the secret is revealed and the share policy is logged |
| "The voting DB is just round state" | `alpha BLOB` in `001_init.sql:28` | **Wrong**: it needs encryption or memory-only |
| "Start with interactive signing" | One-tap shares are readable by every member, so any member could aggregate and front-run | Holds, now with a reason |

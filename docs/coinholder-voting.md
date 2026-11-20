# Coinholder voting for vaults

Research, 2026-11-19. Nothing here is built yet. Upstream moves fast, so check versions
before building on this.

## How the protocol works

Valar Group's shielded voting runs on a separate vote chain (a Cosmos chain; the validators
are listed in the dynamic voting config). It was used for the NU7 coinholder vote
(snapshot 3,459,350, voting closed 2026-09-14). Vizor, ZODL, Zkool and Keystone support it.
On 2026-11-19 no round was open; the latest test round, "[TEST] NU7.1 Poll", closed on
2026-11-16.

1. **Delegation.** The holder proves it owned unspent **Ironwood** notes at the snapshot
   (Halo 2 "ZKP1", up to 5 notes per bundle, 0.125 ZEC per ballot). That voting weight goes
   to a *hotkey* as a Vote Authority Note (VAN) on the vote chain.
   - Funds authorise this with **one RedPallas SpendAuth signature per bundle**. It is made
     under `rk = ak + [alpha]G` over the sighash of a never-broadcast "TX1".
   - TX1 is one Ironwood action spending a dummy note. Its `rho` binds the bundle's note
     commitments, the VAN and the round, so the signature authorises only that delegation.
2. **Vote.** The hotkey spends the VAN and casts an encrypted, split ballot ("ZKP2", signed
   with the hotkey's own key).
3. **Shares and tally.** Submission servers reveal the shares, and validators decrypt only
   the totals.

Spec: the draft ZIP [zcash/zips#1200](https://github.com/zcash/zips/pull/1200), from
August. It is older than the code: it still says Orchard, BIP 39 hotkeys and a sighash that
isn't rebuilt. Trust the crate's behaviour, not the draft. Service discovery is
[zcash/zips#1244](https://github.com/zcash/zips/pull/1244).

## Why a vault fits

The only thing that needs spend authority is the delegation signature. That is the same
operation we already run with FROST: a rerandomized signature under the vault's `ak` with an
`alpha` fixed in advance. Every other input comes from the FVK, which every member holds
(`nk`, `rivk`, note witnesses at the snapshot).

Hardware wallets already sign this way: Keystone in ZODL and Vizor. The library hands out a
`DelegationSigningRequest { sighash, alpha, .. }` (`zcash_voting` 5.4.0 `src/delegate.rs:908`)
and takes back a detached signature (`PreparedSigner::signature`, `:962`).

## How ZODL does it

- `zodl-inc/zcash-android-wallet-sdk` `backend-lib/Cargo.toml` uses
  `zcash_voting = { version = "=5.3.0", default-features = false, features = ["lrz"] }`.
  It runs on **stable** librustzcash: `zcash_client_backend =0.25.0`, `pczt =0.10.0`,
  `orchard 0.16`.
- The crate's default feature is `zakura`, Valar's fork stack. ZODL says it would add
  about 4.2 MB of duplicate crypto. `lrz` resolves to the same versions as the rest of the
  wallet.
- Orchard gains `unstable-voting-circuits` through `zcash_voting`.
- Keystone delegation goes through `keystoneSigningRequests` / `storeKeystoneSignatures`
  (iOS `VotingCryptoClientInterface.swift`).
- The hotkey is a random 64-byte secret kept per wallet (`StoredVotingHotkey`).
- The config is a hash-pinned static document that points to a signed dynamic config
  (`docs/voting-service-discovery.md` in zodl-ios).
- With Tor on, everything a poll does goes over Tor.

## What blocks us

- **Version pins.** Zafe pins the NU7 *pre-releases* (`zcash_protocol =0.11.0-pre.0`,
  `zcash_client_backend =0.25.0-pre.1`, `zcash_client_sqlite =0.23.0-pre.1`,
  `pczt =0.10.0-pre.1`). The stable releases are out: `zcash_protocol 0.11.0`,
  `zcash_primitives 0.31.1`, `zcash_keys 0.17.0`, `zcash_client_backend 0.25.0`,
  `zcash_client_sqlite 0.23.0`, `pczt 0.10.0`. A probe crate that depends on `zafe-core`
  and `zcash_voting =5.4.0` with `lrz` fails to resolve on `zcash_protocol`. **First step:
  move to the stable releases** (worth doing on its own).
- **Votes are signed by one key only.** `VoteSigner` has a single variant, `Hotkey`
  (`src/vote.rs:2339`; it is `#[non_exhaustive]`). ZKP2 derives the spending key from the
  hotkey seed (`src/zkp2.rs:45`). The circuit only needs `ak`, `nk`, `rivk` and a
  randomizer, so this is an API limit, not a circuit limit.

## Who casts the vault's vote

Delegation is final for the round: once a note's governance nullifier is published, it
cannot be delegated again. After that, whoever holds the hotkey casts the vault's full
weight.

- **A. Delegate to one member's (or an outside delegate's) hotkey.** This works with 5.x
  today.
  - t members approve "round R, N ballots → this hotkey address".
  - The vault builds the delegation with `DelegationKeys::with_round_bound_voting_target`
    (`src/delegate.rs:211`). It hands the VAN data over with
    `export_delegation_capability` / `import_delegation_capability`
    (`src/delegation_capability.rs:274`, `:387`), sealed on our relay.
  - The delegate then votes alone and sees how the vault votes.
- **B. The vault votes as a whole.** The VAN goes to the vault's own address (the draft
  allows hotkey = holder keys). Each ballot is a proposal: t approve the answers, and FROST
  signs the vote sighash.
  - Needs upstream: a `VoteSigner` for an external signer, and a ZKP2 built from the FVK
    instead of a seed.
  - `alpha_v` and the share randomness could come from the vault secret, so every member
    derives the same values.

## What a member checks before signing a delegation

This is a new kind of thing signed with the vault key, so it gets its own verifier next to
`verify::verify_pczt`, never a sighash handed over by someone else.

- Rebuild TX1 from the governance PCZT and **compute the sighash locally**.
- TX1 must be exactly one Ironwood action, NU6.3 branch id, expiry 0, value balance +1 zat,
  no other bundles (`src/tx1.rs:1`).
- The spend is the dummy note on a vault address, and its `rho` is recomputed from:
  - the bundle's `cmx`s, each a vault note this member's wallet holds at the snapshot;
  - the VAN, recomputed from the approved target, the ballot count and the shared blinding;
  - a round authenticated against the pinned voting config.
- The output goes to the approved target.

`verify_pczt` already rejects TX1 (branch id, expiry), so a delegation can't pass as a
payment. A TX1 can't pass as a payment the other way either: its only spend is a dummy note,
and its branch id is no longer current on mainnet.

**`alpha` stays inside the vault**, as for payments: the member who proves ZKP1 needs it,
and the vote chain sees only `rk`.

## Other work

- **Vault log.** Add delegation proposals and approvals as new vault events (a
  `VAULT_EVENT` bump, gated on every member). Only logged approvals release shares (rule 5).
  Start with interactive signing only, not one-tap pools.
- **Network.**
  - Fetch the lightwalletd tree state ourselves through `wallet::connect` and pass it with
    `DelegationLwdInputs::from_anchor_tree_state` (`src/delegate.rs:508`). Don't use
    `gather_delegation_lwd_inputs`, which dials its own channel.
  - Vote chain, PIR and helper HTTP go through the `RouteHttp` trait
    (`src/http_transport.rs:307`), routed through our Tor choke point.
  - Config is ZIP 1244 discovery with the pinned hash. No telemetry: the crate's
    observability stays off.
- **Witnesses.** Witnesses at the snapshot come from the lightwalletd frontier plus
  `WalletDb::generate_ironwood_witnesses_at_historical_height` (`src/witness.rs:200`). They
  don't depend on our 100 checkpoints.
- **Not verified yet** (needs a regtest spike):
  - ZIP 2005 (`use_qsk`) vault FVKs and notes go through ZKP1;
  - external and internal (change) notes need separate bundles, since one bundle needs one
    `ivk`;
  - the proving time of ZKP1 on our phones.

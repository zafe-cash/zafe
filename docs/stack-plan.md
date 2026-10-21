# Zafe stack plan: become the shared multisig layer for Zcash

Goal: when any Zcash wallet (Zodl, Vizor, Zkool, Zallet...) adds shielded multisig, the
obvious path is to build on the stack Zafe put out:
- ZF's FROST for the cryptography;
- our vectors and our checks, upstreamed into ZF and librustzcash where they belong;
- Zafe's open coordination protocol and crates for everything a multisig needs on top.

The Zafe app becomes one skin over that stack.

Started 2026-10-01. The research and reasoning behind every item are in
`docs/stack-design.md` (§ numbers below point there). Legend as in `tracker.md`: `[ ]` open,
`[x]` done, **(you)** needs the user.

**Principles**
- **Upstream first.** Anything ZF or ECC would own (DKG, ciphersuite, FVK constructor,
  PCZT verifier) goes to them as review, vectors and small PRs, not as a rival
  implementation.
- **Own the "brain" in the open.** The coordination layer (vault log, proposals,
  approvals, note reservation, commitment pools, leader resumability, invites, backups)
  is Zafe's. It ships as crates with an MIT/Apache spec and test vectors.
- **Fit the host wallet's shape.** The host keeps its DB, sync, PCZT building, proving
  and broadcast; we never duplicate them. A vault looks like a Keystone account: import
  the UFVK, build a PCZT, get it back signed.
- **Never block on upstream.** ZF and ECC take months on big changes. Ship ours
  versioned, then migrate.
- **Small, coordinated PRs.** Ask on an issue first, keep PRs split, use Conventional
  Commit titles. That is how aryaethn got merged in 4 days.

---

## Phase 0: home and hygiene (week 1)

- [x] GitHub org **`zafe-cash`** (free, matches zafe.cash; `zafe` is taken by a user);
      the repo moved there as `zafe-cash/zafe` on 2026-10-03 (GitHub redirects the old
      `AnmolBansalDEV/zafe` links). Keep the app and crates in one repo until a second
      consumer exists.
- [ ] **(you)** Reserve the crate names on crates.io once the first crate is publishable.
      Free on 2026-10-01: `zafe-protocol`, `zafe-proto`, `zafe-coordinator`,
      `zafe-relay`, `zafe-core`, `zafe-vault`, `zafe`. Don't publish placeholder crates:
      crates.io discourages name squatting.
- [ ] `CONTRIBUTING.md` (Conventional Commits, as frost/frost-tools use; how to run every
      test), `SECURITY.md` (`security@zafe.cash`; matches the site's security.txt),
      PR and issue templates. Keep MIT OR Apache-2.0: it's what ZF uses, and it links
      into Zodl's AGPL SDK.
- [ ] CI: add `cargo deny` (licences, advisories). Consider `cargo vet`, since frost-tools
      requires it for new dependencies.
- [x] **Fix the `sk` last-contributor bias** (done 2026-10-01: `DKG_ROUND1` 3,
      `keygen::check_contribution`, test `a_contribution_changed_after_seeing_the_others_is_rejected`). Commit to
      `H(r_i)` in DKG round 1 (bump `DKG_ROUND1`), reveal after the DKG, abort on a
      mismatch. Add a test where a member changes its contribution after seeing the
      others'. Update spec §7.4 and AGENTS.md.

## Phase 1: upstream quick wins (weeks 1-3, in parallel with phase 2)

Each item: ask or comment first where noted, then a small PR. Record links in `tracker.md`
"Upstream".

- [ ] **ZIP 2005 `use_qsk` vectors** (conradoplg invited them on frost#1094). Opened
      frost-tools#609 (issue) and #610 (PR, 2026-10-04): `zcash-sign/tests/fixtures/
      zip2005_use_qsk.json` plus a Rust test written from the ZIP text; the Python
      checker is linked, not included. Awaiting review. *Our first
      merged upstream PR.* (Replaces the tracker item that targeted zcash-test-vectors;
      a generator there is a later follow-up.)
- [ ] **FROST book docs PR**: `book/src/zcash/technical-details.md` says to throw `sk`
      away, which contradicts ZIP 2005 (already on the tracker).
- [ ] **Review frost-tools #579/#580** ("confirm PCZT contents"; open since 2026-01, no
      reviews). Leave a review that points out the checks a co-signer needs (§3 row 3),
      and offer follow-up PRs.
- [ ] **frost#1094**: offer the PR that un-deprecates `sign()` (or adds
      `sign_with_randomizer`), with docs that α must stay among the signers. Then drop
      our `#[allow(deprecated)]` wrapper.
- [ ] **zips#895 review**: (a) a builder-chosen α as an explicitly allowed option, (b)
      our commit-then-reveal fix for the bias str4d raised, (c) an offer to cross-check
      the `sk` derivation with our vectors.
- [ ] **frost-tools #433** (send group info and threshold in the DKG): design posted on
      the issue 2026-10-04; implemented and tested on `zafe-cash/frost-tools`
      `feat/dkg-send-threshold` (frost-client only: the creator sends `{threshold,
      description}` encrypted before Round 1; not pushed). PR once conradoplg answers.
      Book docs follow-up in the frost repo.
- [ ] **frost-tools #488** (2-input PCZT signing): a regression test, for reputation.
- [ ] **zcash-test-vectors #130** (Ironwood v6 sighash): cross-check with our code and
      post the result as a review.
- [ ] **(you)** Introduce the plan in ZF's Discord `#frost`: one message linking the
      vectors PR and the crate-split issue, and asking what they'd like upstream first.

## Phase 2: split the crates (weeks 2-6)

Ten steps, each a merge that keeps `cargo test --workspace` green (§2). Each step also
updates AGENTS.md's layout and commands.

- [ ] 1 (S) Move `ZafeNetwork` and `NoteHold` into protocol-side modules
      (`network.rs`), re-exported from `wallet`.
- [ ] 2 (S) `material.rs` + `codec.rs` inside zafe-core: `VaultMaterial`, `Invite`, the
      request/commitment hashes and codecs, the wire structs, `expectations`,
      `note_holds`, `pool_target`, `forget_closed*`, `is_ready`, `review`, the proving
      keys. Gives `backup` its own error type and removes its dependency on node.
- [ ] 3 (M) Traits in `coordinator.rs` (`Mailbox`, `VaultLog`, `Membership`,
      `VaultWalletOps`, `ChainClient`, `LeaderStore`, `SentTxStore`, `Runtime`),
      implemented for today's types. Node functions become generic. Typed
      `TransportError`/`WalletError` keep the bridge's error kinds. The insufficient-funds
      string match becomes a typed variant.
- [ ] 4 (M) **In-memory harness** (MemoryRelay with compare-and-swap and the 403 on
      self-addressed messages, MockWallet, MockChain), plus a default (not ignored) test
      of propose → approve × t → request → respond → finalize, one-tap `send_ready`,
      and the `NotesInUse` rebuild. Today these flows only run in Docker tests.
- [ ] 5 (M/L) Pull sans-IO steps out of node (`build_vote`, `aggregate_ready`,
      `answer_request`, `classify_inbox`, `filter_shares`, `proposal_event`), then the
      `KeygenCeremony` state machine.
- [ ] 6 (M) Move the leader's resumable send (`proposals.rs:805-937` and the CLI's copy)
      and `maintain()` into the coordinator. `propose` verifies its own PCZT before
      logging it; used commitments are persisted before a request is sent.
- [ ] 7 (S) Move `history.rs` (CSV) into the app bridge.
- [ ] 8 (L) Physical split, part 1: `crates/zafe-protocol`, with zafe-core as a facade.
      Move the protocol tests and vectors. CI: `cargo check -p zafe-protocol
      --no-default-features` (no tokio, tonic, rusqlite or reqwest).
- [ ] 9 (M) Physical split, part 2: `zafe-net`, `zafe-relay-client`,
      `zafe-wallet-sqlite`, `zafe-coordinator`, `zafe-backup`. SQLCipher only in the
      wallet crate.
- [ ] 10 (S) Library crates on semver ranges (`orchard = "0.16"`, `pczt = "0.10"`, ...);
      exact pins only in the app lockfile. rustdoc on every public item, `examples/`
      showing a host wallet driving one payment.

## Phase 3: the open spec (weeks 4-10)

- [ ] `docs/protocol/`: a spec written so a second implementation can interoperate,
      covering every item in §5. Wire formats, signatures and AEADs, DKG ceremony order,
      signing requests, **vault-log replay rules** (these work like consensus),
      verification rules, expiry, the safety words algorithm, backups.
- [ ] Test vectors beyond ZIP 2005: envelope and log-entry encodings, request hashes,
      safety words, and **log → state replay vectors** (a sequence of entries and the
      resulting `VaultState`), with a Python checker like the ZIP 2005 one.
- [ ] Specify the frostd transport profile (interactive signing only; §4).
- [ ] **Forum post** (*Shielded multisig coordination: a shared spec*). Invite Konclave
      (deegalabs), aryaethn, Hanh (Zkool) and pacu (FROST UniFFI SDK) to review or
      co-author.
- [ ] **Draft ZIP**: `draft-<owner>-shielded-multisig-coordination` (category 300-399),
      once the forum thread has settled. Include the FROST backup format if aryaethn
      hasn't started one; otherwise align with his.

## Phase 4: deeper upstream work (months 2-4, ask before each)

- [ ] **librustzcash `pczt` semantic Verifier** (ZIP 374 role). Open an issue proposing
      `check_payments(ufvk, expected)` + change ownership + exact fee from our
      `verify.rs`. If ECC agrees, PR it and make `zafe-protocol` call it. Also use or
      propose the ZIP 374 **Redactor** path to strip α from any exported PCZT.
- [ ] **Quantum-recoverable FROST FVK**: ask conradoplg about orchard#475; offer
      `from_ak_and_sk_zip2005` plus vectors, then switch `zcash-sign generate` (frost-tools
      #591 kept the non-recoverable constructor).
- [ ] **frostd**: a `/wait`-style long poll (#266) and optional persistence, so the
      relay features we need live upstream. Plus share repair in frost-client (#311).
- [ ] **`FrostdMailbox` adapter** in our stack (interactive rounds over frostd; the log
      stays on a `VaultLog`), so a vault can run against ZF's server.
- [ ] **COCKTAIL-DKG**: review frost#1032, cross-check the `COCKTAIL(Pallas,
      BLAKE2b-512)` vectors, prototype the zips#895 `sk` payload layer. When it ships, a
      versioned `KeygenCeremony` v2 on it; old vaults keep v1.
- [ ] **redpallas move (frost#963)**: offer help; switch from reddsa 0.5.2 the day it ships.

## Phase 5: bindings and the first host wallet (months 2-5)

- [ ] `zafe-uniffi` (Kotlin/Swift) over the coordinator, plus traits for host-provided
      PCZT building, nonce storage (platform keystore) and transport. Check pacu's FROST
      UniFFI SDK first and reuse or align with it.
- [ ] **Zodl Android proof of concept**: their SDK pins identical librustzcash versions
      and already has the Keystone PCZT flow (§6). Fork their SDK and demo app privately:
      a Cargo feature in `backend-lib` depending on our pure crates, a handful of JNI
      functions, one 2-of-3 testnet payment. Video plus a write-up.
- [ ] **(you)** Take the POC to ZODL. Their SDK is AGPL with a commercial licence, so
      this is a conversation about how they'd ship it, not a cold PR.
- [ ] Vizor: a `zakura` Cargo feature (renamed deps: zakura-orchard, zakura-pczt,
      zakura-reddsa, ff/group 0.14) plus a CI matrix entry, *only if* Chainapsis is
      interested. First verify PCZT byte compatibility between our pczt (0.10.0-pre.1) and zakura-pczt.
- [ ] A CLI participant built on zcash-devtool's PCZT commands + `zafe-protocol` (a
      low-risk demo for Zallet/devtool users).
- [ ] Rebuild the Zafe app's bridge purely on the public crates (the skin proves the stack).

## Phase 6: audit, adoption, funding (months 3-9)

- [ ] **Audit scope**: `zafe-protocol` + `zafe-coordinator` + the spec (+ relay). Get a
      Least Authority quote. **(you)** Ask ZCG to fund it via its security lead. An
      audit of shared infrastructure is an easier ask than "another wallet".
- [ ] **Flagship pilot (you)**: offer the lockbox key-holders (ZF, ZODL, Shielded Labs)
      a testnet vault. The keyholder-organizations draft and ZIP 271 expect a FROST
      address.
- [ ] Mainnet after the audit; then a coinholder retroactive grant (comparable winners
      got $35-80k) with on-chain use and the integrations as evidence.
- [ ] Measure "canonical": wallets using the crates, ZF citing the spec, merged upstream
      PRs. Revisit this plan quarterly.

## Decisions waiting on you

- [ ] **(you)** Org name (`zafe-cash` proposed) and moving the repo.
- [ ] **(you)** Whether to invite Konclave and aryaethn to co-author the spec. That's the
      faster route to "canonical", at the cost of shared ownership.
- [ ] **(you)** Whether to approach ZODL (AGPL + commercial) or Chainapsis (Apache, but
      on the Zakura fork) first. The plan assumes ZODL, since its stack is identical.
- [ ] **(you)** Whether the relay stays a Zafe server or we push its features into
      frostd (Phase 4). Both are possible; frostd upstream is better for "canonical",
      while our relay keeps a paid hosted tier.

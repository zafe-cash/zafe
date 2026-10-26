# Review: Zcash Shielded Treasury Spec (`spec.v0.md`)

**Date:** 2026-09-29
**Reviewed:** the original hackathon spec, now [`spec.v0.md`](spec.v0.md) (§1–§28). Every section number below refers to that file, not to the current [`spec.md`](spec.md), which was rewritten from this review.
**Verdict:** Direction is sound; the spec is not yet buildable as written. The main problems are omissions around key setup (DKG plus the ZIP 2005 shared secret), trust in the coordinator during setup, and what signers must verify. Before building, also account for two existing projects that already implement most of this spec on Ironwood.

This review is a record of 2026-09-29. Its "coordinator", "treasury" and "signer" are the old spec's terms (in `spec.md`: relay, vault, member).

---

## 0. Summary

### What the spec gets right

- It builds on PCZT as the single transaction object (§9) and refuses a parallel signing format.
- It uses audited FROST and doesn't implement cryptography itself (§6, §15, §23).
- It keeps identity keys separate from FROST shares (§4).
- It says outright that the policy engine is not the security boundary (§19).
- It refuses blind signing: signers must display and validate the transaction (§10, §14).
- It targets Ironwood, which has been the live shielded pool since NU6.3 (July 28, 2026, block 3,428,143). Orchard is now withdrawal-only.
- It says not to advertise production-grade custody until recovery is designed (§17).

### Top issues, by priority

| # | Severity | Issue | Sections |
|---|---|---|---|
| 1 | Critical | Treasury creation leaves out the ZIP 2005 shared secret `sk` / `qsk` | §5, §6, §7, §17 |
| 2 | Critical | The coordinator can sit in the middle of the DKG and the `sk` agreement, which lets it spend | §5, §6, §13 |
| 3 | Critical | The spec doesn't say where the wallet engine runs or who holds the viewing key, so the privacy claims can't be checked | §8, §9, §22 |
| 4 | Critical | Signers must check the sighash and randomizer against the PCZT, not trust the coordinator | §11, §14 |
| 5 | Critical | Signers must check change outputs | §10, §14 |
| 6 | Critical | Removing a signer doesn't revoke their spend power (unless shares are refreshed) or their viewing and `qsk` access | §16, §26 |
| 7 | High | The model has one signing session per proposal, but one FROST signature is needed per spend, including dummy spends | §11 |
| 8 | High | The message protocol is incomplete and partly redundant | §13 |
| 9 | High | Three state machines don't match each other | §11, §12 |
| 10 | High | Rejection, signer-set and restart rules are missing | §10, §11 |
| 11 | High | Concurrent proposals can pick the same notes | §9, §12 |
| 12 | Strategic | Konclave and Quorum already build this product; the §28 positioning doesn't set it apart | §1, §26, §28 |

---

## 1. Ironwood context the spec should record

The spec says "Ironwood" throughout but never cites the specifications that define it. Add a "Protocol references" section:

| ZIP | What it defines | Why it matters here |
|---|---|---|
| ZIP 258 | Deployment of NU6.3 (Ironwood) | Activation height and network parameters |
| ZIP 229 | v6 transaction format | Ironwood bundle; anchors moved into authorizing data (see §3.5 of this review) |
| ZIP 2005 | Ironwood quantum recoverability | Changes key derivation for FROST wallets (see §2.1); all Ironwood notes use the lead byte `0x03` plaintext format |
| ZIP 312 | FROST for spend authorization signatures | Re-randomized FROST; rules for the coordinator and signers |
| ZIP 317 | Proportional fees | Fee checks on the signer side |

Relevant facts:

- Ironwood reuses the Orchard action structure, Halo 2 and RedPallas spend authorization, so re-randomized FROST (`frost-rerandomized` / RedPallas) applies. It has its own note commitment tree, nullifier set and value pool.
- Wallet announcements (Zodl) say Ironwood uses the existing Orchard receiver. For a new FROST treasury the receiver comes from the ZIP 2005 derivation described below. The spec's phrase "Ironwood-compatible address derived from the group spending authority" (§7) is too loose.
- The Orchard → Ironwood migration and turnstile are not relevant to a new treasury, but the spec should say so explicitly. It should also say whether the product accepts deposits sent to the Orchard receiver.

---

## 2. Critical issues

### 2.1 Treasury creation leaves out the ZIP 2005 shared secret

**Spec (§5, §6, §7):** treasury creation = FROST DKG → group public key → address.

**Problem:** An Ironwood address and viewing key are not derived from the FROST group key alone. ZIP 2005 specifies the following for FROST wallets:

- `ak` comes from the FROST DKG (the joint spend-authorization public key).
- Participants must **privately agree on a value `sk`** and derive with `use_qsk = true`:
  - `qsk = H^qsk(sk)`: the quantum spending key
  - `qk = H^qk(qsk)`: the quantum intermediate key
  - `nk`: derived from `sk` as usual
  - `rivk = H^rivk_ext_qk(ak, nk)`: this binds the viewing key to `ak`, `nk` and `qk`
- "All participants obtain the same values for `ak`, `nk`, and `rivk_ext`."
- "Each participant MUST treat `qsk` as a secret held at least as securely and reliably as `ask`."
- Wallets must keep, or be able to re-derive, `sk`, and must record whether `use_qsk` was used.

**Consequences for the spec:**

1. **Setup has two parts, and the spec describes only one.** It needs (a) a FROST DKG for `ask` shares and (b) a confidential agreement on `sk` among participants. Specify how `sk` is agreed. For example, one participant generates it and encrypts it to every other participant's identity key, or it's derived from a contribution by each participant.
2. **`sk` is not threshold-protected.** Every signer holds all of it, and it's needed for `qsk`. Anyone holding `sk` has full viewing capability, and `qsk` is the secret that recovery depends on. Spending still requires `t` signatures verifiable by `ak`, so `sk` alone cannot spend. The spec's statement that each signer holds "one share" (§2 security table) is incomplete.
3. **Data model changes.** Suggested:

   ```typescript
   interface Treasury {
     id: string
     network: "testnet" | "mainnet"
     threshold: number
     participantCount: number

     groupPublicKey: Uint8Array   // ak
     ufvk: string                 // unified full viewing key (Ironwood/Orchard component)
     unifiedAddress: string
     birthdayHeight: number       // where scanning starts
     useQsk: true                 // always true for FROST treasuries (ZIP 2005)

     createdAt: number
   }

   interface SignerSecrets {       // signer device only, encrypted at rest
     groupId: string
     participantId: string        // FROST Identifier
     threshold: number
     keyPackage: Uint8Array       // FROST KeyPackage (ask share)
     sk: Uint8Array               // ZIP 2005 shared secret → nk, qsk, qk, rivk
   }
   ```

4. **The backup contents need to be defined (§17).** A usable backup is the FROST key package, plus `sk` (or at least `qsk` and the full viewing key), plus the `use_qsk` flag, plus the birthday height. Say this explicitly. "Offline backup" in §17 is currently undefined.

### 2.2 The coordinator can sit in the middle of setup and end up able to spend

**Spec (§2, §13):** all traffic goes through the coordinator. §13 says messages "must be authenticated to participant identities" but never says how participants learn each other's identity keys.

**Attack:** During treasury creation the coordinator gives Alice fake identity keys for Bob and Charlie, then runs the DKG with Alice while impersonating them. The coordinator ends up with two of the three shares, which is enough to spend. The same attack works on the `sk` agreement from §2.1. That breaks the only must-have security item in §25, "coordinator cannot spend independently".

**The FROST DKG also assumes:**

- a **broadcast channel**: every participant must receive the same round-1 packages. A coordinator that sends different packages to different participants breaks this.
- **confidential, authenticated point-to-point channels** for round-2 packages.

**Required additions:**

- **Identity bootstrap:** participants confirm each other's identity public keys out of band (QR code, spoken fingerprint or safety number, or an invitation link shared outside the coordinator) *before* the DKG starts. The signer UI must block the DKG until every peer fingerprint is confirmed.
- **Round 2 end to end:** round-2 DKG packages and the `sk` agreement are encrypted to the recipient's identity key. The coordinator only relays ciphertext.
- **Broadcast consistency:** after round 1, each participant publishes a hash of every round-1 package it received, signed with its identity key. Everyone compares the hashes and aborts on any mismatch.
- **Transcript binding:** at the end of the DKG, all participants sign `(groupId, ak, ufvk, participant list, threshold)` with their identity keys. The treasury is valid only with all N signatures. Signers later refuse signing requests for a treasury without this transcript.
- **FROST identifiers:** derive each participant's FROST `Identifier` from their identity public key (`Identifier::derive`) so the two can't be separated.

### 2.3 The spec doesn't say where the wallet engine runs

**Spec (§8, §9):** "The wallet engine constructs a PCZT." §8 draws "Signer / wallet engine → lightwalletd", and §22 calls the coordinator "non-custodial but privacy-sensitive".

**Problem:** Building a PCZT and scanning the chain both need the full viewing key (`ak`, `nk`, `rivk`, and the note data). Orchard/Ironwood proving needs no spend secret, but it does need full knowledge of the spends. Whichever component builds PCZTs can therefore see every incoming and outgoing treasury transaction.

**Decide explicitly and document the trade-off:**

| Option | Who holds the full viewing key | Privacy | Complexity |
|---|---|---|---|
| A. Wallet engine on the proposer's signer device (WASM) | Signers only | Best: coordinator sees only ciphertext | Scanning and Halo 2 proving in the browser; each device syncs |
| B. Wallet engine as a helper next to the coordinator, holding a view-only UFVK | Signers plus helper | Helper sees all transaction data | Simple clients; this is Konclave's "Architecture B" |
| C. Self-hosted helper per organization | Signers plus the org's own server | Good if the org runs it | Operational burden on the customer |

If you choose B for the hackathon, change §2 and §22 to say so plainly: "the coordinator/helper can view all treasury activity but cannot spend." Don't claim privacy from the coordinator.

### 2.4 Signers must check the sighash and randomizer against the PCZT

**Spec (§11, §14):** signers verify "PCZT integrity", and the coordinator runs the rounds.

**ZIP 312 facts:**

- The coordinator generates the randomizer (`randomizer_generate()`, hashed from fresh randomness and the signing package) and sends it with the message and commitments over a confidential, authenticated channel.
- The message being signed is the transaction sighash, which on its own tells a signer nothing. ZIP 312: "signers MUST check that the given SIGHASH matches the data sent from the Coordinator, or compute the SIGHASH themselves from that data."

**Required signer checks for each signing package:**

1. Parse the PCZT locally with the `pczt` crate and **compute the v6 sighash locally** (ZIP 229, including the Ironwood digest). Refuse if the signing package's message differs.
2. For the spend being signed, check that the randomizer `α` in the signing package matches the PCZT spend: `rk == ak + [α]G`, with `rk` from the PCZT action.
3. Check that the spend belongs to this treasury (`ak` matches the stored group key).
4. Check that the PCZT's consensus branch ID and network match the treasury.
5. Run all the output and fee checks in §2.5 before sending round 1.

Without checks 1 and 2, a malicious coordinator can show Alice "10 ZEC to vendor" while asking her to sign a different sighash.

### 2.5 Signers must check change outputs

**Spec (§14):** the checklist is "treasury ID, recipient, amount, fee, network, proposal ID, PCZT integrity".

**Problem:** A transaction can pay the stated recipient the stated amount and still send the rest of the spent notes to the attacker, labelled as "change". Every output needs to be accounted for.

**Required checks:**

- **Every** output is one of: (a) a recipient in the proposal with exactly the proposed amount and memo, (b) change to **this treasury's own address** (checked by decrypting with the treasury's viewing key), or (c) a zero-value dummy output.
- Sum of inputs − sum of outputs = fee, and the fee matches ZIP 317 within a set tolerance.
- Transparent outputs and outputs to other pools are either refused or shown explicitly.
- Show the full output list in the approval UI, not only the "main" payment.

Also add "no other outputs" and "change returns to treasury" to §14.

### 2.6 Removing a signer doesn't revoke what matters (§16, §26 second demo)

**Spec:** "Charlie leaves → Dave joins → FROST resharing → same treasury → no funds moved."

**Problems:**

1. **Spend power.** Giving Dave a share (for example with FROST's repairable threshold scheme) doesn't invalidate Charlie's share. Charlie plus any one old-share holder can still sign. Revocation needs a **share refresh** among the remaining signers, plus **deletion of the old shares**. The deletion can't be proved, so this is a trust assumption.
2. **Viewing and `qsk`.** Charlie still has `sk`, and with it the full viewing key and `qsk`. Charlie keeps permanent visibility into all past and **future** treasury transactions. Resharing can't change this: `nk`, `rivk` and `qsk` are fixed for the address. The only real revocation is to create a new treasury (new DKG, new `sk`) and move the funds, which is the "migration" the demo says it avoids.
3. **Hostile vs. friendly removal.** Friendly rotation (the signer cooperates and deletes their data) and hostile removal (a compromised or departed signer) need different procedures. Define both.

**Required changes:**

- Split §16 into *rotation*, which keeps the address and assumes the old signer deletes their data, and *revocation*, which requires a new treasury and moving the funds.
- Add the viewing-key caveat to §16, §18 and the demo narrative.
- In §16, name the actual `frost-core` mechanism (repair and/or refresh) and check which participant-set changes it supports.

---

## 3. High-severity protocol gaps

### 3.1 One signature per spend, including dummy spends (§11)

- Each Ironwood action with a real spend needs its own spend authorization signature, with its own randomizer `α`.
- An Ironwood-era engine also creates **zero-value dummy spends controlled by the wallet** that need FROST signatures. Konclave fixed exactly this bug: they now collect every spend with `spend_auth_sig().is_none()` instead of filtering by value.
- So a proposal needs **k** FROST signatures, where k is the number of spends to sign. Run all k within one session, with one set of commitments and one signing package per spend, so signers approve once.

Update `SigningSession` to track each spend:

```typescript
interface SigningSession {
  id: string
  proposalId: string
  pcztHash: Uint8Array              // binds the session to one exact PCZT
  signingSet: ParticipantId[]       // fixed after round 1
  spends: {
    actionIndex: number
    randomizer?: Uint8Array
    commitments: Record<ParticipantId, Uint8Array>
    shares: Record<ParticipantId, Uint8Array>
  }[]
  status: SessionStatus
  expiresAt: number
}
```

### 3.2 Message protocol (§13)

Problems with the current `Message` union:

- `FROST_ROUND_2` and `SIGNATURE_SHARE` are the same thing: the round-2 output *is* the signature share.
- `SIGNING_PACKAGE` is missing (coordinator → signers: commitments, message, randomizer, one per spend).
- There are no DKG messages at all (DKG round 1 broadcast, DKG round 2 point-to-point encrypted, transcript confirmation), and no `sk` agreement message.
- No message carries the sender, recipient, identity signature, session binding or sequence number.
- `REJECT` has no way to show it came from the signer.

Suggested outline:

```typescript
type Envelope = {
  v: 1
  treasuryId: string
  sessionId: string
  from: ParticipantId
  to: ParticipantId | "broadcast"
  seq: number
  body: Uint8Array          // encrypted to `to` for point-to-point; plaintext only if public
  sig: Uint8Array           // identity-key signature over all of the above
}

type Body =
  // setup
  | { type: "DKG_R1"; package: Uint8Array }
  | { type: "DKG_R1_ECHO"; hashes: Record<ParticipantId, Uint8Array> }
  | { type: "DKG_R2"; package: Uint8Array }              // encrypted to recipient
  | { type: "SK_SHARE"; payload: Uint8Array }            // ZIP 2005 sk agreement, encrypted
  | { type: "DKG_TRANSCRIPT_SIG"; transcriptHash: Uint8Array }
  // signing
  | { type: "PROPOSAL"; pczt: Uint8Array; proposal: PaymentProposal }
  | { type: "APPROVE"; proposalId: string; pcztHash: Uint8Array }
  | { type: "REJECT"; proposalId: string; pcztHash: Uint8Array; reason?: string }
  | { type: "COMMITMENTS"; perSpend: Uint8Array[] }
  | { type: "SIGNING_PACKAGE"; perSpend: { message: Uint8Array; randomizer: Uint8Array; commitments: Uint8Array }[] }
  | { type: "SIGNATURE_SHARES"; perSpend: Uint8Array[] }
  | { type: "ABORT"; reason: string }
```

Before designing your own, check whether `frostd` and `frost-client` already provide this envelope and encryption layer (see §5).

### 3.3 The state machines don't match (§11, §12)

The spec has three lifecycles that don't line up:

- `SigningSession.status`: pending, round_1, round_2, complete, rejected, expired
- `ProposalStatus`: never defined
- The §12 PCZT lifecycle: Created → Partially signed → Threshold satisfied → Finalized → Broadcast → Confirmed

Problems:

- "Partially signed" doesn't exist in FROST. Signature shares combine into one RedPallas signature per spend. Nothing is ever partially signed on-chain.
- There are no failure states: invalid share or aggregation failure, broadcast rejected, dropped from the mempool, transaction expired, reorg.
- Proposal approval (a human decision) and signing (a cryptographic round) are mixed together.

Suggested single `ProposalStatus`:

```
draft → pending_approval → approved (≥ t approvals)
      → signing → signed → broadcast → mined → confirmed(N)
side exits: rejected | expired | cancelled | signing_failed | broadcast_failed | tx_expired
```

A signing session is a child of a proposal and can be retried (for example after a signer drops) without creating a new proposal.

### 3.4 Rejection and signer-set rules (§10, §11)

Rules to define:

- **Rejection threshold:** a proposal fails when `N − t + 1` signers reject, because after that it can't reach the threshold. One reject in a 2-of-3 is not enough.
- **Signing set:** fixed at the moment the coordinator builds the signing package from round-1 commitments. If a member of the set drops before sending round 2, the session aborts and restarts with a new set. Commitments from the aborted session are discarded, never reused.
- **Cheater identification:** the coordinator verifies each signature share individually before aggregating (FROST supports this) and records which participant sent an invalid share.
- **Approval ≠ signature:** an approval is a signed intent from the identity key. The FROST share is what actually authorizes. The UI should show both separately.

### 3.5 Concurrent proposals and note selection (§9, §12)

- Two pending proposals can select the same notes. When the first one lands, the second becomes invalid because the nullifier is already spent.
- Required: note reservation (lock the selected notes to a proposal until it's confirmed, expired or cancelled) and a clear "insufficient unreserved balance" error.
- **Anchors (improved by ZIP 229):** v6 moves anchors into authorizing data, so a transaction can be re-anchored and re-proved after signing without changing the txid or invalidating the signatures. Approvals can take hours or days. State this in the spec as a design advantage. Also check how the `pczt` crate exposes re-anchoring (see librustzcash issue #2529 on pre-signed v6 chains).
- Tie `Proposal.expiresAt` to the transaction's `expiryHeight`, and show both.

### 3.6 FROST nonces vs. proposal nonce (§9, §15)

- `PaymentProposal.nonce` (an application ID) and FROST signing nonces (secret, single-use) are different things. Rename the first to `clientRequestId` or remove it.
- Replaying a signed transaction on-chain is already impossible because nullifiers are unique. The replay risks that matter are (a) a signer being tricked into approving the same payment twice and (b) **FROST nonce reuse**.
- Rules for nonces:
  - Never persist round-1 secret nonces to durable storage (browser-extension storage sync and restore is a real hazard).
  - If the signer restarts between round 1 and round 2, abandon the session.
  - Use the `frost-core` `SigningNonces` lifecycle, which consumes the nonces on use.
- The signer keeps a local log of `(proposalId, pcztHash)` it has signed and warns about duplicate payments (same recipient, amount and memo within a time window).

---

## 4. Medium issues

### 4.1 Policy engine (§19)

- A policy enforced only by the coordinator can be bypassed by anyone who controls the coordinator. It only means something if **each signer checks the policy before signing**.
- `Policy.threshold` duplicates the FROST threshold. It can only be stricter than `t`, never looser. Rename it to something like `approvalThreshold` and document that constraint.
- `dailyLimit` needs a spend history that signers trust. Signers can rebuild it from the treasury's own chain history using the full viewing key, which they hold (§2.1).
- The policy itself must be signed by the treasury (for example, all N identity keys at setup, and `t` to change it), or a malicious coordinator can swap it.

### 4.2 Viewing and audit authority (§18)

- Because of ZIP 2005 (§2.1), **every signer already has full viewing capability**. "Separate spend from view" only applies to *additional* roles such as an auditor or accountant.
- Give auditors an incoming viewing key (IVK) when they only need incoming data, and the full viewing key only when they need outgoing data too. Viewing access can't be revoked; say this in the UI.
- The "Employee → proposal only" role needs neither viewing nor spend keys, just coordinator access control. That's an application-level permission, not a cryptographic one.

### 4.3 Data types and units

- `PaymentProposal`, `Proposal` and the SDK's `PaymentRequest` overlap. Define one canonical type in `packages/types`.
- `amount: bigint`: specify **zatoshis**.
- Memo: at most 512 bytes and only valid for shielded recipients. Define what happens for transparent and TEX (ZIP 320) recipients: refuse them, or allow them with a warning and no memo.
- Recipient validation: parse unified addresses, choose the receiver (Ironwood/Orchard preferred), and check the network prefix.

### 4.4 Signer storage (§3.3, §4)

- "Encrypted at rest" needs a key source: a passphrase (Argon2id) or the OS keychain in the desktop app. In a browser extension, `chrome.storage` plus a passphrase is the minimum.
- Wipe secrets from memory after use. Konclave writes unsealed shares only to ephemeral `0600` tmpfs files during signing, which is a good reference.
- The browser extension is the main attack surface. List it in §21.

### 4.5 Security model additions (§21)

Add:

- **Coordinator during setup:** can sit in the middle of the DKG and `sk` agreement without the §2.2 mitigations. This is currently the biggest hole.
- **Coordinator or helper holding the full viewing key:** full loss of privacy (§2.3).
- **Removed signer:** keeps viewing access forever, and spend ability until the shares are refreshed (§2.6).
- **Malicious signer during the DKG:** FROST DKG aborts but doesn't continue without the bad participant. It's a denial of service, not a compromise. Document how a failed DKG is restarted.
- **Compromised proposer device:** can build a malicious PCZT. The signer checks in §2.4 and §2.5 are the defence.
- **Compromised lightwalletd:** can hide notes or lie about confirmations. Consider checking against a second server.

### 4.6 Transaction pipeline (§2 diagram, §9)

The diagram shows "Final PCZT → lightwalletd". Map the PCZT roles to components explicitly:

| PCZT role | Component |
|---|---|
| Creator / Constructor | Wallet engine (see the §2.3 decision) |
| IO Finalizer | Wallet engine |
| Prover (Halo 2) | Wallet engine; decide between browser WASM and server |
| Spend Finalizer / Signer (FROST aggregate injected) | Coordinator aggregates; signature injected into the PCZT |
| Combiner | Coordinator |
| Transaction Extractor | Coordinator |
| Broadcast | Coordinator → lightwalletd `SendTransaction` |

### 4.7 Dependencies and version pinning

From Konclave PR #259. This is secondhand, so verify before relying on it:

- `pczt` 0.9.1, `zcash_client_backend` 0.24.0-rc.6 and related librustzcash crates on the Ironwood line.
- `orchard` pinned to a git revision with the `unstable-frost` feature, because the crates.io 0.15.5 release reportedly drops the patch needed to extract the spend-auth data for FROST.
- `frost-tools` at a revision that includes `zcash-sign` v6 signing (frost-tools #593).

I haven't confirmed lightwalletd's Ironwood (v6) support. Check it before committing to §8.

---

## 5. Scope for the hackathon

The §25 must-have list is long, and §23 adds infrastructure a hackathon doesn't need.

**Cut from the hackathon:**

- Prometheus and Grafana
- Postgres (SQLite or in-memory is enough)
- The policy engine (already listed as a stretch goal; keep it out)
- Docker, unless the judges need it

**Reuse instead of building:**

- **ZF `frost-tools`:** `frostd` is a coordination server for FROST participants, `frost-client` runs DKG and signing through it, and `zcash-sign` signs PCZTs with an external (FROST) signature and supports v6/Ironwood. Together these cover most of §3.2 and §13.
- **`zcash-devtool`** for building and inspecting PCZTs during development.

**Must-have list, rewritten:**

1. Out-of-band identity verification, then a 2-of-3 DKG plus `sk` agreement, then a signed transcript
2. Ironwood testnet address and balance
3. Proposal → PCZT with note reservation
4. Signer shows **all** outputs, recomputes the sighash, checks the randomizer
5. FROST signing across all spends, including dummy spends
6. Aggregate, extract, broadcast, confirm
7. Demonstrate that the coordinator's database and logs contain no share and no `sk`

---

## 6. Strategy: existing projects

Two projects already implement most of this spec on Ironwood.

### Konclave (deegalabs/konclave), Apache-2.0/MIT

- A local-first treasury with FROST threshold signing, a Vite/React UI, a Rust orchestrator and a Tauri desktop app.
- **DKG runs in the browser** (compiled to WASM) over a **blind relay** that only forwards public or already-encrypted bytes.
- **Helper server** ("Architecture B"): builds PCZTs from a view-only UFVK, waits for signatures, injects them and broadcasts. Option B in §2.3.
- Uses unmodified `frostd`, `frost-client`, `zcash-sign`, `zcash-devtool` and librustzcash.
- **Mainnet:** 2-of-3 payments, private payroll (many outputs with encrypted memos), Orchard → Ironwood migration and FROST spends from Ironwood, with signing in the browser across two machines.
- Shares sealed at rest (XChaCha20-Poly1305 + Argon2id), secrets wiped from memory, share repair (repairable threshold scheme) proven in tests.

### Quorum (Fatihmaull/zcash-multisig)

- A Next.js web app, a coordinator with no key material, and `frost-core` v3.0.0 with re-randomized FROST.
- Confirmed 2-of-3 Ironwood spend on testnet. Described as a release candidate, testnet only.
- **Entered in the Colosseum Crypto World's Fair Zcash track (Sept 14 – Oct 12, 2026; submissions due Oct 10).** Its stack and pitch are close to this spec.

### What this means

- §28's pitch ("programmable shielded treasury infrastructure", "Safe-like private treasury") describes what both projects already claim.
- The hackathon demo in §26 (2-of-3 → DKG → address → fund → propose → sign → broadcast) is already on mainnet in Konclave.
- **Differentiator options** (pick one or two and build the spec around them):
  1. **Dapp SDK / provider API (§20):** a `window.zcash`-style provider plus an SDK that lets apps propose payments to a treasury. Neither project leads with this.
  2. **Policies enforced by signers** (§4.1): spending limits and allow-lists that each signer checks, so they hold even against a malicious coordinator.
  3. **Hardware signers:** Ledger's Zcash app has pull requests for Ironwood (NU6.3 / v6) PCZT signing. A FROST participant on a Ledger device would stand out, but it depends on Ledger's firmware work.
  4. **Proper rotation vs. revocation** (§2.6), with automatic migration to a new treasury on hostile removal.
  5. **Privacy from the coordinator:** a wallet engine running only on signer devices (option A in §2.3), which Konclave's helper architecture doesn't provide.
- Also consider contributing to or forking Konclave (permissive license) and spending the effort on the differentiator instead of rebuilding the base.

---

## 7. Minor and editorial

- Heading levels are inconsistent: §1–§2 use `##`, §3 onward use `#`.
- §2's diagram skips PCZT extraction and doesn't show the wallet engine or prover. Add them after the §2.3 decision.
- §20 imports from `@yourorg/zcash-sdk`, a placeholder. Pick the package name.
- §24's repository layout has `crates/frost` and `crates/pczt`. Since the spec forbids reimplementing either, these should be thin wrappers or removed in favour of upstream crates.
- §27 puts "recovery" in Phase 3, but §17 says not to advertise custody without it. Keep the two consistent in the pitch materials.
- The "Status" line says "MVP / Hackathon → Startup". Add a version number and a date.

---

## 8. Sources

- ZIP 2005, Ironwood Quantum Recoverability: https://zips.z.cash/zip-2005
- ZIP 312, FROST for Spend Authorization Multisignatures: https://zips.z.cash/zip-0312
- ZIP 229, Version 6 Transaction Format: https://zips.z.cash/zip-0229
- ZIP 258, Deployment of the NU6.3 Network Upgrade: https://zips.z.cash/zip-0258
- zcash/ironwood (formal verification, Ironwood book): https://github.com/zcash/ironwood
- The Block, Ironwood activation: https://www.theblock.co/post/409934/zcash-ironwood-upgrade-launching-new-shielded-pool-after-orchard-vulnerability
- CoinDesk, Ironwood goes live: https://www.coindesk.com/tech/2026/07/28/zcash-seals-usd1-7-billion-shielded-pool-as-ironwood-upgrade-activates
- Zodl, Ironwood: A New Shielded Pool: https://zodl.com/ironwood-a-new-shielded-pool-for-zcash/
- ZcashFoundation/frost-tools: https://github.com/ZcashFoundation/frost-tools
- Konclave: https://github.com/deegalabs/konclave
- Konclave PR #259 (Ironwood PCZT bump, dummy spend fix): https://github.com/deegalabs/konclave/pull/259
- Quorum / zcash-multisig: https://github.com/Fatihmaull/zcash-multisig
- LedgerHQ app-zcash PR #35 (Ironwood / V6 support): https://github.com/LedgerHQ/app-zcash/pull/35
- librustzcash #2529 (pre-signed v6 transaction chains via PCZTs): https://github.com/zcash/librustzcash/issues/2529

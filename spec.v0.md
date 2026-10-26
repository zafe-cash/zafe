> **Superseded by [`spec.md`](spec.md).** This is the original hackathon spec, kept as history; [`spec-review.md`](spec-review.md) reviews it.

# Zcash Shielded Treasury — Technical Specification

**Status:** MVP / Hackathon → Startup  
**Target:** Ironwood shielded pool  
**Core primitives:** FROST, PCZT, lightwalletd  
**Primary use case:** 2-of-3 shielded treasury with independent signers

---

## 1. Product

A non-custodial, Safe-like treasury for shielded Zcash.

A user creates a treasury controlled by `M-of-N` independent signers.

Example:

```text
Treasury
├── Threshold: 2-of-3
├── Alice
├── Bob
└── Charlie
```

A payment requires two signers.

```text
Alice ── approve ──┐
                   ├──> FROST ──> valid spend
Bob ─── approve ───┘

Charlie ── reject / unavailable
```

The coordinator never holds enough information to spend the funds.

### Long-term product

The product should evolve into programmable shielded treasury infrastructure supporting:

- 2-of-3, 3-of-5, and other M-of-N configurations
- payment proposals
- signer approval/rejection
- transaction history
- viewing/audit access
- signer replacement
- policy rules
- dapp integration
- hardware/mobile signers

---

## 2. Architecture

```text
                         ┌───────────────────┐
                         │    Treasury UI    │
                         │                   │
                         │ proposals         │
                         │ balances          │
                         │ history           │
                         │ signers           │
                         └─────────┬─────────┘
                                   │
                                   │ HTTPS
                                   ▼
                         ┌───────────────────┐
                         │   Coordinator     │
                         │                   │
                         │ sessions          │
                         │ PCZT transport    │
                         │ signer routing    │
                         │ policy engine     │
                         └─────────┬─────────┘
                                   │
                         authenticated/encrypted
                                   │
                  ┌────────────────┼────────────────┐
                  │                │                │
                  ▼                ▼                ▼
             ┌─────────┐      ┌─────────┐      ┌─────────┐
             │ Signer A│      │ Signer B│      │ Signer C│
             │         │      │         │      │         │
             │ FROST   │      │ FROST   │      │ FROST   │
             │ share A │      │ share B │      │ share C │
             └─────────┘      └─────────┘      └─────────┘
                  │                │                │
                  └────────────────┼────────────────┘
                                   │
                                   ▼
                              Final PCZT
                                   │
                                   ▼
                              lightwalletd
                                   │
                                   ▼
                                Ironwood
```

### Security boundaries

| Component | Role | Holds spending key material? | Trust level |
|---|---|---:|---|
| Treasury UI | UX / proposals / history | No | Untrusted |
| Coordinator | Routing / sessions / aggregation | No | Untrusted for custody; privacy-sensitive |
| Signer | FROST signing | Yes, one share | Security-critical |
| FROST group | Threshold spend authority | Distributed | Cryptographic security boundary |
| Ironwood | Settlement | N/A | Consensus layer |

---

# 3. Components

## 3.1 Treasury frontend

Web application.

Responsibilities:

- create treasury
- display shielded balance
- create proposals
- show pending approvals
- display signer status
- display transaction history
- manage policies
- initiate signer rotation
- integrate with dapps

**Must never contain FROST secret shares.**

Suggested stack:

```text
Next.js
TypeScript
Tailwind
```

---

## 3.2 Coordinator

A backend that coordinates signing sessions.

### Responsibilities

- treasury metadata
- participant discovery
- signing sessions
- PCZT transport
- FROST round coordination
- signature-share collection
- PCZT finalization
- broadcasting
- audit events

Example API:

```text
POST /treasuries
GET  /treasuries/:id

POST /proposals
GET  /proposals/:id

POST /signing-sessions
GET  /signing-sessions/:id

WS   /sessions/:id

GET  /transactions/:id
```

### Critical security property

The coordinator must **never possess**:

- FROST private shares
- spending keys
- a reconstructed private key

If the coordinator is compromised, the attacker must not be able to independently spend treasury funds.

---

## 3.3 Signer

The signer is the security-critical component.

For the MVP, implement a browser extension or standalone signer application.

Future signer implementations:

```text
Browser signer
Mobile signer
Hardware signer
CLI signer
```

### Responsibilities

```text
generate identity key
generate FROST share
participate in DKG
receive signing request
validate transaction
produce FROST signature share
participate in resharing
```

### Critical rule

The FROST share never leaves the signer device.

---

# 4. Identity vs FROST key

Keep these separate.

```text
Signer
├── Identity key
│     └── authenticates device
│
└── FROST share
      └── authorizes Zcash spends
```

Example:

```typescript
interface SignerIdentity {
  participantId: string
  publicKey: Uint8Array
}

interface FrostShare {
  groupId: string
  participantId: string
  threshold: number
  share: Uint8Array // encrypted at rest
}
```

The `share` must never be sent to the coordinator.

---

# 5. Treasury creation

User selects:

```text
Threshold: 2
Participants: 3
```

Flow:

```text
Treasury UI
     │
     ▼
Create treasury
     │
     ▼
Generate group/session ID
     │
     ▼
FROST DKG
     │
 ┌───┼────┐
 ▼   ▼    ▼
 A   B    C
 │   │    │
 └───┼────┘
     ▼
Group public key
```

Each participant gets their own FROST share.

The resulting group public key becomes the treasury's spend authority.

---

# 6. Distributed Key Generation (DKG)

Use an existing audited Zcash FROST implementation.

**Do not implement FROST cryptography from scratch.**

The DKG should support:

```text
2-of-3
3-of-5
M-of-N
```

For the hackathon, implement:

```text
2-of-3
```

first.

---

# 7. Address generation

The treasury needs an Ironwood-compatible receiving address derived from the appropriate group spending authority.

Do not invent a new address or key derivation scheme.

Use the current Ironwood-compatible Zcash libraries and primitives.

Example treasury record:

```typescript
interface Treasury {
  id: string
  network: "testnet" | "mainnet"

  threshold: number
  participantCount: number

  groupPublicKey: Uint8Array

  unifiedAddress: string

  createdAt: number
}
```

---

# 8. Balance and wallet state

Shielded balances are wallet-specific.

Do not model Zcash like Ethereum:

```text
GET /balance?address=...
```

Instead, the treasury wallet needs to maintain the relevant wallet state/viewing capability.

Conceptually:

```text
Signer / wallet engine
       │
       ▼
lightwalletd
       │
       ▼
scan Ironwood
       │
       ▼
treasury notes
```

For the MVP, use the current Ironwood-compatible wallet libraries rather than implementing wallet scanning and note management from scratch.

---

# 9. Payment proposal

User enters:

```text
Recipient: u1...
Amount: 10 ZEC
Memo: Invoice #123
```

Frontend creates a proposal:

```typescript
interface PaymentProposal {
  treasuryId: string

  recipient: string
  amount: bigint
  memo?: string

  nonce: string
}
```

The wallet engine constructs a **PCZT**.

### Design rule

PCZT is the canonical transaction object passed between components.

Do not invent a parallel transaction/signing format.

---

# 10. Approval flow

```text
Alice
  │
  │ create proposal
  ▼
PCZT
  │
  ▼
Coordinator
  │
  ├─────────────► Bob
  │
  └─────────────► Charlie
```

Each signer displays enough transaction information to make an informed approval.

Example:

```text
┌──────────────────────────┐
│ PAYMENT REQUEST          │
│                          │
│ Amount       10 ZEC      │
│ Recipient    u1...       │
│ Memo         Invoice 123 │
│                          │
│ Treasury    Company XYZ  │
│                          │
│ 2 of 3 required          │
│                          │
│ [ Reject ]   [ Approve ] │
└──────────────────────────┘
```

**Never ask a signer to approve a blind opaque blob.**

The signer should independently validate the transaction using the available Ironwood/PCZT tooling before contributing a signature.

---

# 11. FROST signing session

Example session:

```typescript
interface SigningSession {
  id: string
  proposalId: string

  participants: Participant[]

  threshold: number

  status:
    | "pending"
    | "round_1"
    | "round_2"
    | "complete"
    | "rejected"
    | "expired"
}
```

Conceptual flow:

```text
Round 1

Alice ───── commitment ────►
Bob   ───── commitment ────►
Charlie ── no response

                    ↓

Round 2

Alice ───── signature share ──►
Bob   ───── signature share ──►

                    ↓

              threshold reached
                    ↓
              aggregate/finalize
```

The coordinator aggregates signature shares but never receives the underlying FROST shares.

---

# 12. PCZT lifecycle

PCZT should have an explicit lifecycle:

```text
Created
   ↓
Partially signed
   ↓
Threshold satisfied
   ↓
Finalized
   ↓
Broadcast
   ↓
Confirmed
```

Example:

```typescript
interface Proposal {
  id: string
  treasuryId: string

  pczt: Uint8Array

  status: ProposalStatus

  approvals: Approval[]

  createdAt: number
  expiresAt: number
}
```

---

# 13. Coordinator ↔ signer communication

For MVP:

```text
HTTPS
+
WebSocket
```

Example:

```text
wss://coordinator.example/session/{sessionId}
```

Messages:

```typescript
type Message =
  | {
      type: "SESSION_INIT"
      sessionId: string
    }
  | {
      type: "PCZT_REQUEST"
      pczt: Uint8Array
    }
  | {
      type: "FROST_ROUND_1"
      payload: Uint8Array
    }
  | {
      type: "FROST_ROUND_2"
      payload: Uint8Array
    }
  | {
      type: "SIGNATURE_SHARE"
      payload: Uint8Array
    }
  | {
      type: "REJECT"
      reason: string
    }
```

Messages must be authenticated to participant identities.

The transport is replaceable. Future transports could include:

```text
WebSocket
HTTPS
QR
Bluetooth
LAN
libp2p
Nostr
```

Keep transport separate from the cryptographic protocol.

---

# 14. Don't trust the coordinator

The signer should independently verify:

```text
treasury ID
recipient
amount
fee
network
proposal ID
PCZT integrity
```

The coordinator saying:

> "Alice approved 10 ZEC"

is not sufficient.

Alice's signer must produce the cryptographic signature contribution.

---

# 15. Replay protection

Every proposal needs unique identifiers.

Use:

```text
proposalId
sessionId
nonce
expiry
```

The signer should maintain sufficient state to prevent accidental or malicious replay.

### Important

FROST nonce handling is security-critical.

Do not implement nonce generation or reuse prevention yourself unless there is a compelling reason. Use the audited FROST implementation's APIs and lifecycle.

---

# 16. Signer rotation

Initial treasury:

```text
2-of-3

Alice
Bob
Charlie
```

Replacement:

```text
2-of-3

Alice
Bob
Dave
```

Preferred approach:

```text
old group key
      │
      ▼
FROST resharing
      │
      ▼
new participant shares
      │
      ▼
same group authorization
```

The desired property is:

```text
old group public key == new group public key
```

if the selected resharing scheme supports it.

This allows signer replacement without migrating the treasury funds.

---

# 17. Recovery

Recovery must be explicitly designed.

Example:

```text
2-of-3

Alice
Bob
Charlie
```

Bob loses his device:

```text
Alice + Charlie
       │
       ▼
reshare
       │
       ▼
Alice + Charlie + Dave
```

If Alice and Bob both disappear:

```text
Charlie
```

is below threshold.

Possible future recovery mechanisms:

- recovery participant
- offline backup
- hardware signer
- social recovery
- organizational recovery key
- dedicated recovery process

Do not advertise production-grade custody without a defined recovery mechanism.

---

# 18. Viewing and audit authority

Separate:

```text
spending authority
```

from:

```text
viewing authority
```

Future architecture:

```text
FROST spend authority
          +
viewing/audit authority
```

Potential roles:

```text
CFO
 ↓
spend

Auditor
 ↓
view

Accountant
 ↓
view

Employee
 ↓
proposal only
```

This separation is especially important for shielded treasury use cases.

---

# 19. Policy engine

Implement after the basic signing flow works.

Example:

```typescript
interface Policy {
  threshold: number

  maxTransactionAmount?: bigint

  dailyLimit?: bigint

  allowedRecipients?: string[]

  requiredParticipants?: string[]
}
```

Example policy:

```text
2-of-3
+
≤ 10 ZEC
+
approved recipient
```

Important distinction:

**The policy engine is not the cryptographic security boundary.**

FROST threshold authorization remains the actual spending control.

---

# 20. Dapp SDK

Once the treasury works, expose a developer SDK.

Example:

```typescript
import { ZcashTreasury } from "@yourorg/zcash-sdk"

const treasury = await ZcashTreasury.connect()

await treasury.proposePayment({
  recipient,
  amount,
  memo
})
```

Potential API:

```typescript
interface ZcashTreasuryProvider {
  getTreasury(): Promise<Treasury>

  getBalance(): Promise<Balance>

  createProposal(
    request: PaymentRequest
  ): Promise<Proposal>

  getProposal(
    id: string
  ): Promise<Proposal>

  approveProposal(
    id: string
  ): Promise<void>

  rejectProposal(
    id: string
  ): Promise<void>
}
```

This eventually becomes the dapp connectivity layer.

---

# 21. Security model

## Coordinator compromised

Attacker can potentially:

- censor proposals
- reorder requests
- deny service
- attempt privacy attacks

Attacker must not be able to:

- obtain FROST shares
- independently authorize a spend

## Frontend compromised

Attacker can:

- show malicious proposals
- attempt phishing

Signer must independently display and validate the transaction.

## One signer compromised

For a 2-of-3 treasury:

```text
1 FROST share ≠ spend authority
```

## Threshold signers compromised

For a 2-of-3 treasury:

```text
2 shares = spend authority
```

This is intentional.

---

# 22. Privacy considerations

The coordinator should be treated as **non-custodial but privacy-sensitive**.

Avoid storing unnecessary plaintext transaction data.

Longer-term goal:

```text
Dapp
 ↓
encrypted/authenticated protocol
 ↓
Coordinator
 ↓
Signers
```

The coordinator should eventually be unable to inspect more transaction information than strictly necessary.

For the hackathon, prioritize correctness and cryptographic security first, then improve coordinator privacy.

---

# 23. Technology stack

Recommended:

```text
Frontend
Next.js + TypeScript

Coordinator
Rust or Node.js

Crypto boundary
Rust

Browser integration
Rust/WASM

Database
Postgres

Realtime
WebSocket

Zcash connectivity
lightwalletd

Deployment
Docker

Observability
Prometheus + Grafana
```

### Crypto rule

Keep cryptographic operations in Rust/audited Zcash ecosystem libraries.

Expose a small, typed interface to TypeScript/WASM.

Do not implement FROST, PCZT cryptography, note selection, proof generation, or consensus logic from scratch.

---

# 24. Repository structure

```text
zcash-treasury/
│
├── apps/
│   ├── web/
│   ├── signer-extension/
│   └── coordinator/
│
├── packages/
│   ├── sdk/
│   ├── protocol/
│   └── types/
│
├── crates/
│   ├── zcash-engine/
│   ├── frost/
│   ├── pczt/
│   └── wasm/
│
├── infra/
│   ├── docker/
│   └── postgres/
│
└── docs/
    ├── architecture.md
    ├── threat-model.md
    └── protocol.md
```

---

# 25. MVP Definition

## Must have

- [ ] Ironwood testnet
- [ ] 2-of-3 FROST DKG
- [ ] group public key
- [ ] receiving address
- [ ] balance
- [ ] payment proposal
- [ ] PCZT creation
- [ ] signer approval UI
- [ ] FROST signing
- [ ] PCZT finalization
- [ ] broadcast
- [ ] transaction confirmation
- [ ] signer disconnect/reconnect
- [ ] coordinator cannot spend independently

## Stretch

- [ ] signer rotation
- [ ] QR pairing
- [ ] viewing/audit authority
- [ ] policy engine
- [ ] dapp SDK
- [ ] mobile signer

## Do not build initially

- [ ] custom cryptographic primitives
- [ ] custom Zcash transaction format
- [ ] custom consensus logic
- [ ] full hardware-wallet support
- [ ] every Zcash address type
- [ ] generalized WalletConnect compatibility
- [ ] complex enterprise RBAC

---

# 26. Hackathon demo

The demo should tell one coherent story.

```text
                 CREATE TREASURY

                   2-of-3
              Alice / Bob / Charlie
                       │
                       ▼
                 FROST DKG
                       │
                       ▼
                Ironwood address
                       │
                       ▼
                  FUND TREASURY
                       │
                       ▼
              PROPOSE 10 ZEC PAYMENT
                       │
             ┌─────────┴─────────┐
             ▼                   ▼
          Alice ✓              Bob ✓
             │                   │
             └─────────┬─────────┘
                       ▼
                  FROST SIGN
                       │
                       ▼
                  FINAL PCZT
                       │
                       ▼
                 BROADCAST
                       │
                       ▼
                 IRONWOOD ✓
```

### Killer second demo

```text
Charlie leaves
       ↓
Dave joins
       ↓
FROST resharing
       ↓
same treasury
       ↓
no funds moved
```

The product story becomes:

> **Private Zcash treasury infrastructure with threshold authorization, rather than simply another Zcash wallet.**

---

# 27. Startup roadmap

## Phase 1 — Hackathon

```text
Ironwood
+
PCZT
+
2-of-3 FROST
+
Signer
+
Coordinator
+
Treasury UI
```

## Phase 2 — Developer platform

```text
SDK
+
Signer protocol
+
dapp integration
+
QR/deep links
+
mobile signer
```

## Phase 3 — Treasury platform

```text
Policies
+
audit/viewing
+
signer rotation
+
recovery
+
hardware signing
```

## Phase 4 — Enterprise

```text
RBAC
+
approval workflows
+
spending limits
+
accounting integrations
+
compliance/audit infrastructure
+
multi-organization support
```

---

# 28. Product positioning

Do not position the project primarily as:

> "WalletConnect for Zcash."

That focuses on transport and puts you directly against generic wallet-connectivity efforts.

Position it as:

> **Programmable shielded treasury infrastructure for Zcash.**

The underlying stack is:

```text
FROST
  +
PCZT
  +
Ironwood
  +
Signer protocol
  +
Coordinator
  +
Policy engine
  +
Viewing/audit
  +
Dapp SDK
```

The first application is a Safe-like private treasury.

The long-term platform is the infrastructure that lets applications build around shielded Zcash without taking custody of users' funds.


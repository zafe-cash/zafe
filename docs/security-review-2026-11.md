# Security review, November 2026: root causes and decisions

Sources: Daniel Gorgonha's report (fee padding, fixed in `a57718b`, `f98fc88`) and four
adversarial read-throughs (dishonest member, hostile relay/network, setup and recovery,
relay server and app). Findings were checked against the code before they were accepted.

What held: members only ever sign their own locally computed sighash; shares are bound to
the request; nonces are never reused; the log can't be forged or silently rewritten. What
failed is narrower and repeated: a handful of trust rules were never written down, so each
feature applied them partly. The fixes below follow those rules; patches that don't follow
from a rule are not made.

## The rules

1. **A reveal covers only what was fixed before it.** In a commit-then-reveal check, the
   revealed secret must be bound to the exact counterparts and inputs that were committed
   when it left the phone. A secret that outlives its transcript can be ground against.
2. **Trust flows from the human check.** After two people compare safety words, a device
   accepts only what the person it checked with vouches for (their signed messages), never
   what the relay lists.
3. **Derive, don't store.** Anything shown to a user or used as a target (an address, a
   vault id) is derived from key material or checked against it.
4. **One member's claim can't close, complete or block anything for the others.** Log
   events that change what other members can do need evidence (signatures) or a second
   member. Intent (approve, reject, cancel) is a claim; "this transaction is signed" is a
   fact that needs proof.
5. **Signed stays live until spent or expired.** Cancel and reject change intent, never
   whether a signature may exist. A proposal that may have been signed is treated as able
   to mine until its notes are spent or it expires.
6. **Unknown means stop.** When a phone can't tell where the vault lives, it doesn't write.
7. **The relay verifies and bounds before it locks, and charges before it works.**

## Decisions

### Safety words (rules 1, 2)

- **Setup.** Today the secret is fixed per (identity, vault id), so after a member
  reveals, a relay can switch that member's list and grind the newcomer's value until the
  words match (`node::safety_check`). Decision: derive the secret from the sealed member
  set too: `secret = H(sign(vault id || H(sorted member fingerprints)))`. A different list
  gives a fresh secret whose commitment precedes every newcomer's value, so each view is a
  one-shot guess again, with no new state to store. Not chosen: a durable "pin" file, which
  adds state and restart cases for the same guarantee.
- **Seat recovery.** Today the new phone reveals one secret to the first challenger and
  then shows words for every later challenge, so a late challenger can grind
  (`repair::recovery_checks`). Pinning challenges at reveal would break recoveries that
  need more than one co-signer. Decision: one secret per co-signer, derived from the
  identity and the co-signer's key: the co-signer announces itself, the phone commits for
  that co-signer, the co-signer challenges, the phone reveals for that co-signer only.
  Every reveal is fresh for its counterpart, so every row is a one-shot guess. Stateless.
- **Recovering a fake vault.** Today the recovering phone trusts the relay's member list,
  the last sigma's log key and the relay's log (`repair::recover_from`). Decision: the
  person taps "Words match" on the row of the co-signer they compared with; the phone keeps
  that key and accepts a vault only if that co-signer is a current member, signed the
  approval moving the seat to this phone, and sent a sigma whose anchor checks against the
  log. The relay can't produce messages signed by that co-signer, so it can't invent the
  vault. No vault id in the words: the confirmed key already binds it.
- **Address.** `descriptor.address` is checked against the UFVK when a vault is accepted
  (setup, recovery, restore), and screens show the UFVK's address (rule 3).
- **Recovery slots and repair restarts.** Junk challenges no longer matter to the phone
  (rows are per co-signer and confirmed by tap), so only the relay cap remains: recovery
  writes are charged per IP like creations. A repair restart (`RepairRetry`) takes effect
  only once t current members name the same helpers (rule 4), gated by event version.

Status 2026-11-13 (implemented, not committed):
- Corrected rule: "stateless" was wrong. Any commit-then-reveal needs the revealing side to
  durably remember what it revealed against; a deterministic secret that is re-derived on
  every poll lets a relay re-serve the other side's values ground against it.
- [x] Setup: held via a pin (`safety.pin`: sealed member list + every other commitment,
  written before `SafetyReveal`, deleted by `forget_keygen`); any other list or commitment
  stops the setup. The member-set secret was dropped (redundant with the pin). The value is
  random per attempt (`safety.secret`, own file: written once before the first commit, so
  the pin keeps its single write): a deterministic value came back unpinned after "Leave"
  and rejoining the same invite. Rejoining now commits to a new value and the other
  members, who pinned the old commitment, stop.
- [x] Seat recovery: one check per co-signer, plus a pin per (new identity, co-signer) of
  the commitment and challenge before the reveal; words only over the pinned challenge, a
  swapped challenge refuses the row. A sealed file in the recovery directory rather than
  secure storage: the write-before-reveal step stays inside one core function (as for the
  setup pin) instead of a Dart round trip between pinning and revealing.
- [x] Recovering a fake vault: confirmed co-signer required (current member, approved this
  move, sigma with an anchor that checks; its log key and vault id). "Check every mailbox"
  kept under the same rule.
- [x] Address checked in `start_chain` and backup `validate`; screens use the UFVK's.
- [x] Recovery slots: no cap per new phone (slots per co-signer); co-signers' announce and
  challenge take a `creates_per_ip` token. Deviation: the new phone's commit and reveal are
  not charged (they need that co-signer's announce, so they are bounded by charged writes;
  charging them would let junk announces drain the new phone's own allowance).
- [x] Repair restarts: every named helper (= t members) must log the same list,
  `VAULT_EVENT` 11. Deviation: the app gate waits for v11 from every member except the new
  key (it can't write before its key is back) and the stalled helper (it may never return).
  Replay picks the rule by the entry's tag and the version floor (as for v6-v10), not by a
  replay-derived "every member wrote v11": a dishonest member could keep that gate off for
  good by never writing v11. An app still on v10 restarts at once and diverges, as it
  already ignores every v11 entry and shows "update Zafe".

### Seat moves (rules 2, 4)

Found by the peer review (zafe-28, findings A, B, E; failing tests in
`crates/zafe-core/pending-tests/seat_move_attacks.rs`). The first draft of this section
(pending until the new key's `RepairDone`, any old-key entry voids, relay swaps on
`RepairDone`) was broken in review: the new key alone can log `RepairDone` at once, a
dishonest member can take the old key off the relay so its veto can't be logged, a
blind relay can't see a log event, and a lost phone's background checks write entries
on their own. This is the revised design.

- **A live seat doesn't move.** Today t approvals move a seat at once
  (`ReplaceApproval` → `move_seat`), and the old key's phone isn't told
  (`vault_updates.dart` skips `oldKeyHex == me`). In a 2-of-3, one dishonest member who
  convinces one co-signer that Bob lost his phone gets Bob's share rebuilt onto a key of
  their own, and then signs alone. The safety words don't help: the fooled co-signer
  compares them honestly with the attacker's phone. Decision:
  - **t approvals make the move pending; the seat switches once every helper logged
    `RepairHelped`** for the current attempt. Until then the old key is a member and the
    new key isn't. The new key's word never completes anything (rule 4). One pending move
    per old seat; a later one is ignored while it lasts. The old key can't be a helper
    of its own seat.
  - **Only a `KeepSeat` from the old key voids a pending move.** Background writes (pool
    top-ups, version announcements, kept votes) don't count, so a lost phone that is
    still online can't block a real recovery. The old phone gets push and a Needs you
    card; any normal action there while its seat is pending prompts **Keep my seat**
    (behind unlock).
  - **An honest helper waits 72 h**, timed from when it first saw the t-th approval,
    saved durably (a lost record restarts the wait), never from an entry's timestamp.
    Then it appends `RepairHelped` and checks that its entry replayed as valid (not
    ignored, move still pending); on a conflict it reloads and checks again, never
    resends blindly. Only then does it send its sigma. The deltas between helpers may go
    earlier; the sigma to the new key is the leak. **The last honest helper's sigma is
    the point of no return**: a `KeepSeat` after it voids the move in the log but can't
    undo the leak. What protects the owner is that wait plus "no `KeepSeat` before my
    `RepairHelped`", not the compare-and-swap (a dishonest helper never logs anything;
    it hands its sigma over directly). Assumption: the owner opens the app within 72 h
    of the push.
  - **Enough with one dishonest member:** the helpers are t distinct current members and
    a retry needs every named helper (batch 1), so with fewer than t dishonest members
    at least one helper is honest, and RTS needs every helper's part.
  - **A thief can't keep the seat forever.** After a `KeepSeat`, a new round of t
    approvals is allowed that the same old key can't void. Its approvers see "the old
    phone said it still has this seat on <date>: call them first". That warning is the
    human check that finding A was missing. The wait applies to this round too. Move id
    = H(vault id, old, new, round), round = the seat's `KeepSeat` count so far, so a
    second round is a different move even for the same new key.
  - Gated: new event version; once a current member's entry carries it, every
    `ReplaceApproval` replays under the new rule.
- **The relay follows signatures, never the log (finding B).** The relay is blind; it
  can only check signatures. Today it swaps a member on t approvals, so a dishonest
  member can replay an honest co-signer's approval to the relay alone: the old key loses
  relay access (its `KeepSeat` can't be appended) and every `load_state` fails with
  `RelayMembership`. Decision, all relay-checkable signatures with their own domain
  strings:
  - t approvals make the relay **add** the new key as a *pending* member: inbox reads and
    deliveries to it only; no log appends, no envelopes to others.
  - The relay **removes** the old key only on t signatures over a seat-move finish
    (vault id, move id, old, new), which an honest helper signs together with its
    `RepairHelped`.
  - `KeepSeat` is also a relay cancel signed by the old key over (vault id, move id, old,
    new): the relay drops the pending key and its finish signatures. A move id of a
    later round isn't cancellable, matching the log.
  - A client accepts a relay listing more members than the log only when the log has a
    pending move to that exact key; anything else stays `RelayMembership`.
  - New apps never write the old approval format, so an older app never sees a move
    complete from new approvals.
  - Finish and cancel are relay-only, so they are signed over the label as well as the
    vault id: a finish can't be replayed to the vault's old mailbox after a relay move.
    Approvals stay on the vault id because the log carries them across relays.
- **Older apps and relays: a hard cut.** An older app counts a move complete at t
  old-format approvals and sends its sigma at once, so a mixed vault keeps finding A
  open (review: 2-of-4 with Dave on an old app, Carol gets X added through Alice's
  new-format approvals, then completes the move in Dave's old replay with old-format
  ones). A relay rule could refuse that delivery, but it would stay in the code for
  good to guard one transition. With no users to strand, the release instead bumps
  `RELAY_API` for every body and retires the two routes that take a raw envelope or log
  entry (`/v1/envelope`, `/v1/log/append`: 426 with the version header, not 404) in
  favour of signed wrappers, as `append-labelled` already is. Only `/v1/info` and
  `/health` stay open. An older app gets "update Zafe" from the new relay for
  everything, so there is no mixed vault on it, and a new app gets `RelayOutdated` from
  an older relay, so it never approves a seat move there. Ship the relay first;
  self-hosted relays must update with the app.
- **A repair after a second loss (finding E).** `move_seat` rekeys other unfinished
  repairs' helpers in place without bumping `attempt`, while a helper's part 1 is saved
  per (index, attempt). In a 2-of-3, if C's repair is under way and B's seat then moves
  to B', A's saved part 1 has no delta for B' (`hs.deltas[&B']` panics, `repair.rs`
  ~616). If A already sent it, B' computes a fresh part 1, the two sharings mix, and C'
  fails on every retry. Decision: a helper's seat moving bumps that repair's `attempt`
  and clears its retries (deterministic, gated); one move's error never stops another's;
  a missing delta is an error, never a panic.

Status 2026-11-14 (implemented, not committed; second pass after review):
- [x] A live seat doesn't move (`VAULT_EVENT` 12): t approvals over `SeatMoveApproval`
  (vault id, old, new, round) make the move pending; it switches on the last
  `RepairHelped` of the current attempt; one pending move per seat and per new key; the
  old key never helps its own seat. Gated by tag and floor.
- [x] Helpers: deltas at once; `RepairHelped` after 72 h from their own first sighting
  (`<move id>.seen`, sealed, `durable::create_new`), checked to count; **the sigma only
  once the seat switched** (review fix 1: with sigma-at-`RepairHelped`, a co-helper holding
  back its own part left the share with the new key while the seat still looked pending,
  and the owner's later keep was accepted after the leak). So a valid keep always means no
  honest sigma went out, and after the switch a keep is refused.
- [x] A keep is one signed object (review fix 2): the old key signs `SeatCancel` (label,
  vault id, move id, old, new, at); the log's `KeepSeat` carries it with the label (any
  current member may log it; replay checks it against the old member's key) and the relay
  takes the same signature as the cancel from anyone and shows it in `mailbox_info`.
  `repair::sync_seat_moves` carries a relay-only cancel into the log and a log-only keep to
  the relay, so the old key can't close round 0 on the relay alone and freeze the seat.
  Only the old phone's tap creates the signature.
- [x] `StopHelping` (logged, for good, across attempts, rekeyed with a seat move): that
  helper's part no longer counts and no restart may name it, so it stops a move in any
  round, round 1 included. App: **Stop helping** behind a confirm and the unlock.
- [x] The old phone is told first (push on the first approval and when pending, **Keep my
  seat** card, and every `confirmUnlock` action asks while its seat is pending); members
  see the pending move, their own countdown and who stopped; second-round approvers see
  the keep's date. The app never writes `KeepSeat` or `StopHelping` on its own.
- [x] Relay follows signatures (`RELAY_API` 2, `RELAY_DB` 6): approvals add a pending key
  (receives, reads its inbox, the members, the encrypted log, its mailboxes; never writes);
  t finishes remove the old key; signed cancels close round 0. `/v1/envelope`,
  `/v1/log/append` and `/v1/mailbox/replace` answer 426.
- [x] Finding E: a helper's seat switching bumps the attempt of every repair it helps and
  clears its restarts and logged parts; a saved part 1 for other helpers is an error.
- [x] One move's error never stops another (review fix 3 and item 4): relay sync and first
  sightings collect errors per move and return the first at the end.
- [x] The new phone: "your co-signers wait up to 3 days before finishing" until sigmas
  arrive (it has no log key before), reaches its key before the relay's finish, and its
  writes until then are "still finishing" (only when the log says its seat switched and
  the relay lists it pending for that move), never an error screen.
- Deviation: **members sign a finish only once the log switched the seat**, not at their
  `RepairHelped`: finishes posted earlier let one dishonest helper's finish plus an honest
  one take the old key off the relay while the log stays pending, locking every member out
  (`RelayMembership`). Any current member who read the switch may sign one.
- [x] Relay seat rules are per move (review round 2): a cancel affects only the move it
  names (no seat-wide closed round 0, so a cancelled junk row never strands the real
  move); finishes count on a cancelled row (a thief's cancel after the log switched can't
  strand the seat; a logged keep means no honest finish, so it gives nothing away); the
  pending cap is per seat with its own 409 and message. Only members push approvals.
- [x] Chained moves while the relay lags (review round 3): a seat that moves A -> X -> Y
  while the relay still has A (one member holds its finish) no longer locks members out:
  the membership check walks the seat's switches back to any key the relay still holds,
  and members' refreshes give the relay a switched move's approvals once its old key is the
  relay's member, then their finish, oldest move first.
- [x] The client doesn't check the relay's pending keys any more, only its member list (the
  earlier case list still locked members out when the log dropped an approval the relay
  had a row for). A pending key can't write, reads only ciphertext and its own inbox, and
  honest phones address one only for a log-pending move; tests show an unexplained one
  gets nothing.
- Deviation: the pending key also reads the encrypted log (not only its inbox and the
  member list): `recover_from` must check the log before the relay's finish, and the log
  is ciphertext to anyone without the log key.

### Proposals (rules 4, 5)

- **Possibly signed.** Replay records, once and for good, when a proposal reached Approved
  or got a vote with shares. That flag, not status, drives: wallet holds (its notes stay
  held), the "still sendable" warning, the unapproved-spend alert's grace, and member
  verification: while a possibly-signed cancelled or rejected proposal is unexpired and
  unswept, members approve a new proposal only if it spends one of its notes, so at most
  one of them can mine. The wallet picks such a note itself. This closes the double payment
  and the false "keys compromised" alert after a withdrawn approval. Derived state only, so
  no gate. Not chosen: an automatic sweep, which costs a fee and t signatures.
- **"Sent" needs proof.** A `Broadcast` event carries the signed transaction; replay
  accepts it only if its txid is the PCZT's and every vault spend's signature verifies.
  One member can no longer stop a payment by announcing it. Bonus: every member holds the
  bytes and can resend a dropped transaction. New event version, gated on every member.
- **Reservations need a second member and end.** A proposal reserves notes (`NotesInUse`,
  wallet holds) only once a member other than its author approved it, and never beyond
  the vault's expiry window from the reserving proposal's tip. A lone member can't freeze
  funds. Cost: two honest proposals made at the same moment may pick the same note; one
  mines, the other is reported as dropped. Gated.
- **One-tap and interactive compose.** A member whose vote carried shares may add
  interactive commitments later, so a one-tap approval never strands a payment that needs
  an interactive round. Draining pools then only forces interactive signing. Gated.
- **Signing leaders and the UI.** A member who hasn't voted can always approve while a
  proposal is open or approved. A leader that gets an invalid share or commitment drops
  that member for the round and marks it silent. A phone that released shares won't send
  a reject. Local only.
- **Version floor.** Only current members' entries raise the replay version. Gated.

Status 2026-11-14 (implemented, not committed; `VAULT_EVENT` 12, `PROPOSAL_RULES_FROM`):
- [x] Possibly signed: `ProposalState::possibly_signed`, set on Approved or a vote with
  shares, for good. It drives holds, "still sendable", the spend alert's grace and
  `node::check_vote`: a possibly-signed cancelled/rejected proposal W is live while the
  member's tip < W's expiry + 10 blocks (scaled) and no spend of its notes is that deep
  (`node::still_sendable`); a new proposal is approved only if it spends a note of every
  live W earlier in the log. `propose` spends a wallet note of each live W (round 2, below).
  **Make it unsendable** (a sweep of every live W's notes) stays the user's choice.
- [x] Round 2 (zafe-e4, `tests/vault.rs` `proposal_attacks`):
  - A lone member's own shares no longer reserve notes: `reserves()` needs a current
    approval by a member other than the author (a one-tap vote counts; t = 1: the author's).
    Replay doesn't verify shares, so "possibly signed" never reserves on its own.
  - A W made by one member's own shares can't be dropped (one honest share-poster plus t - 1
    dishonest members can sign it), so it can't stall payments either: `check_vote` asks to
    respend only Ws logged before the proposal, and `propose` spends a wallet note of every
    live W. `wallet::ForcedInputs` wraps the wallet's `InputSource`: `GreedyInputSelector`
    funds the transaction with the whole set `select_spendable_notes` returns and
    recomputes the fee over it (zcash_client_backend `input_selection.rs:1470-1497`), so the
    forced notes are added there (the rest of the target from the others), and
    `select_single_spendable_note` finds nothing while notes are forced (used only under
    `PreferSingle`, `input_selection.rs:1453-1468`). This replaces `propose_transfer`
    (`data_api/wallet.rs:779-967`) minus its ZIP 318 crossing attempt (Orchard-pool notes,
    which vaults don't hold) and input locking (off). `StillSendable` is gone; a W whose
    wallet notes are all spent shallow gives "try again in a few minutes".
  - A dropped broadcast (possibly signed, not mined, not in lightwalletd's mempool, before
    expiry + margin) counts as a W: held under its own owner and respent. One in the
    mempool or mined isn't: nobody pays it again. `ChainView::with_mempool` reads the
    mempool whole (never a lookup), only when a broadcast may be pending, and records the
    absent ones (`ChainView.dropped`), so a cached view never takes a later send for a
    dropped one. Proofs stay unverified in replay.
- [x] Precondition: a share leaves a phone only for a request built from logged votes
  (every signer's commitments are its latest vote's, `check_request_logged` in `respond`
  and `sign_own_shares`). Before, `respond` signed whenever the nonces were there, e.g.
  after the vote's append failed. An approval whose append failed now reposts the stored
  commitments instead of answering "already approved".
- [x] "Sent" needs proof: `VaultEvent::SignedBroadcast { tx }` (appended);
  `tx::check_signed`: parses for the PCZT's branch with nothing left over, no transparent
  part, txid = the PCZT's sighash (equal for v6 without transparent, zcash_primitives
  `sighash_v5.rs:38-40`, `txid.rs:467-480`), every action's spend-auth signature verifies.
  A txid-only `Broadcast` is refused from 12. Only for an Approved proposal: a signed
  cancelled one marked sent would make every phone resend it. Members resend dropped ones
  from the logged bytes.
- [x] Reservations: from 12 only with a non-author approval or possibly signed, and until
  `VaultState::reservation_end` = min(expiry, claimed tip + scaled (window + 96 + 144)).
  Members refuse a claimed tip more than 10 (scaled) above their tip ("sync first") or more
  than the window below. Holds: reserving proposals and the phone's own.
- [x] One-tap and interactive compose: commitments after shares are valid from 12 (shares
  kept). `ProposalState::one_tap_blocked` (no group can complete: a member approved
  interactively) shows **Approve again** to members with shares. An interactive request
  with a pool commitment is refused (`check_request_logged`).
- [x] Finding G: `node::reject` takes this phone's nonces first, then refuses
  (`AlreadySigned`, nothing logged) after one-tap shares (logged or kept), a kept or sent
  reply (an interactive approval in the log whose nonces aren't here) or a claimed nonce
  file. `respond` skips a member whose latest vote is a reject (no logged commitments).
- [x] Finding J (`tests/verify_mutation_fuzz.rs`): Ironwood `value_sum` = fee, other pools'
  value sums 0, flags = `ironwood_v3().default_flags()` (zcash_primitives
  `builder.rs:388-400`); proposals use `OvkPolicy::Custom` with the internal OVK for change
  (`Sender` sets none, zcash_client_backend `data_api/wallet.rs:2225-2232`; Ironwood change
  takes it, `wallet.rs:2531-2536`), and every output to the vault must be recovered with it
  as exactly the change address, value and an empty memo. Anchor: not checked (below).
- [x] `FileNonceStore::take` claims by renaming to a per-take name, then reads and deletes.
- [x] Spend alert (follow-up): a spend whose txid is the PCZT-fixed txid of a
  possibly-signed proposal is accounted for at any time, whatever its status; the 24-block
  grace is gone (it covered only the moments before a broadcast entry, which the txid now
  covers). A cancelled or rejected one mined shows as **Sent anyway** (`sent_anyway`).
- [x] Reject (follow-up): "our logged approval without its nonces here" refuses only once
  the proposal is possibly signed, so a restored phone (no nonces) can reject a proposal
  nobody can have signed.
- [x] Leaders: a share that doesn't verify ends the round with the culprit named
  (`InvalidShares`), marked silent, round files dropped; unusable logged commitments are
  skipped and marked silent. The app shows **Approve** to a member who hasn't voted while a
  payment is Open or Approved but not complete.
- Deviation: the per-take claim name (`<file>.<random>.taking`, not `<file>.taking`): with
  one name a second take could replace the file a first one is still reading.
- Deviation: `reject` also refuses when the log holds this member's interactive approval,
  the proposal is possibly signed, and this phone has no nonces for it (a reply was sent,
  or another phone of the member holds them): "reply sent" leaves no other trace.
- Testnet: logs where dev builds wrote a version 12 txid-only `Broadcast` now ignore it; the
  proposal drops back to Approved and its notes stay held (possibly signed) until expiry.

### Relay moves (rule 6)

- A phone that can't read its move notices, or can't tell whether its own move switched,
  refuses to write instead of writing to the old relay.
- A phone that wrote to the old relay after a verified switch can still follow: entries
  after the switch's base on the old relay are void by rule, so following drops them.
- The mover is named in the signed move target, not inferred from the order a relay
  serves arrivals, and a cancel goes to both relays.

### Relay server (rule 7)

- Approvals are deduplicated, capped at the member count and verified before the lock;
  reseed verifies outside the lock with a per-request cap.
- Responses stop at a byte budget and entries have a size cap, so no member can make a
  page too big to read; storage has a global cap; Compose sets memory limits.
- No one member can fill a vault's storage (an inbox, its messages, its log) for
  everyone; a stranger's fresh mailboxes can't take an established vault's room.
- Pushes: bounded concurrency, a per-device rate, tokens that fail are pruned.
- A replayed signed object charges its signer only when it changes something.
- Cross-host redirects are refused; only the creator appends entry 0; Caddy's body limit
  matches the relay's.

Batch 4 (2026-11-14; failing tests in `crates/zafe-relay/tests/rule7.rs`,
`inbox_flood.rs`, `shares.rs`, `quotas.rs`, `crates/zafe-core/tests/relay_redirect.rs`,
`relay_pages.rs`, `proposal_rules.rs`), as built:
- **Seat-move approvals**: a repeated key is 400 and every signature is checked against its
  own key before the lock; under it, more approvals than members is 400, then t verified
  current members other than the old key count, as before.
- **Reseed**: at most `RESEED_MAX_ENTRIES` (512) entries per request, their summed size
  checked, decoded, verified and hashed before the lock; under it only the chain against
  the head and the inserts. The answer's `len` says where to continue (clients already
  do); `open` only once every entry was taken. A new mailbox's room check and creation
  charge happen before the lock.
- **The log length** is `MAX(idx) + 1` through the primary key, never a `COUNT(*)` walk
  (an append on a 200,000-entry log cost 28x one on a short log); cancel setup uses
  `EXISTS`.
- **Pages** stop at `READ_BUDGET_BYTES` (16 MiB) of stored entries or envelopes, at least
  one per page; entries and envelopes are at most `MAX_ENTRY_BYTES` (below). Relay and
  client measure the same thing (`zafe_proto::relay::page_bytes`), and a client's
  end-of-log rule is `page_may_continue`: `len >= READ_PAGE || bytes > BUDGET -
  MAX_BODY_BYTES` (no stored item, old ones included, is larger than a body). The inbox already read until an empty page. An older core (a dev build
  of 37ac433) reads a cut page as the end of the log and, if its saved copy already holds
  more, reports `RelayRolledBack`. Honest logs never reach 16 MiB in 500 entries, so only a
  log someone padded can cut a page; on 37ac433 that log gave every reader
  `ResponseTooLarge` for good instead.
- **Global storage**: `ZAFE_RELAY_MAX_DB_MB` is the database's `max_page_count`; a write
  past it is 507 (`Quota::Relay`, "this relay is full"). From 90% new mailboxes stop
  (checked before the creation token is charged) and mailboxes created here in the last
  30 days stop writing (`YOUNG_MAILBOX_DAYS`, NEW 8). Compose caps each relay's memory
  (768 MiB) and processes, and Caddy's.
- **Pruning** reads only expired rows (`deliveries_by_age`) in batches of 1,000, each its
  own transaction (`DELETE ... RETURNING`, then grouped counter updates).
- **Inbox counters (and NEW 6)**: `inbox_counts` holds count and bytes per (mailbox,
  recipient, sender), changed in the same transaction as every insert, ack and prune of
  `deliveries`, and in `delete_mailbox`. The quota check reads one counter row per
  recipient and the sender's rows (a broadcast into deep inboxes walked ~63 x depth index
  entries). **NEW 6** (from the review): the inbox cap counted every sender's deliveries,
  and phones never ack kinds they don't handle, so one member could fill another's inbox
  (about 33 min at the hosted rate) and every send to it got 507 for 30 days: the leader's
  signing request aborted the round, repair parts to a pending key were blocked, and the
  victim re-downloaded the junk on every read. Caps are now per (sender, recipient) pair,
  fixed: 256 envelopes and 2 MiB hosted. A first cut shared the vault's caps out per pair
  (`/ (members - 1)`, `/ (members x (members - 1))`); the second review showed that is
  42 MiB per pair in a 2-of-3 (a victim reading three 16 MiB pages per call, past the 60 s
  timeout on a slow link: it can't sign) and 66 KB in a 64-member vault (less than one
  signing request). A fixed pair holds two of the largest envelopes, and a victim reads at
  most 2 MiB of one sender's junk. A sender may use at most half the vault's 64 MiB (never
  less than a pair), so one dishonest member can't fill it; a 1/n share (round 2 of the
  review) left a 64-member vault's leader 2 MiB, less than one interactive round with ~20
  notes and t = 33 (~70 KB to each of 32 signers).
- **A refused signer** (NEW 6, client): only a pair's 507 names one recipient (body exactly
  `RECIPIENT_FULL`); a 403 means it isn't a recipient there. The leader marks that signer
  silent and rebuilds the round without it (a request naming it can't complete, and each
  one sent consumes its signers' nonces, so they would approve again for nothing); refused
  signers are tried first, so a set with no reachable t signers fails before anyone else
  got a request, and a signer that got an abandoned request stays used (`Leader::release`
  frees only the others). Any other 507 (the vault's or relay's storage, the leader's own
  total) fails the round: a full relay must not mark every signer silent.
- **NEW 7, the log** (second review): the log cap was per mailbox only, the relay can't
  judge an entry's ciphertext and replay ignores junk, so one member could fill the 512 MiB
  log in under two minutes and freeze the vault for good (the log is kept forever), and a
  move copied the junk into the new relay's total. An author may now append at most half
  the log (`log_counts` per (mailbox, author); `quota::author_log_cap`): one dishonest
  member freezes nobody, and a vault where one person proposes every payment still gets
  half (a 1/n share would stop a 2-of-3 treasurer at a third, years before the total).
  Reseed takes a signed history whole but counts it per author, so a move gives no fresh
  share (k moves would otherwise let one member write k halves).
- **NEW 8, a stranger** (second review): with 2 GiB per network and 768 MiB per mailbox,
  three fresh mailboxes from one IP filled a relay in minutes and froze every vault on it
  (before batch 4: the disk). Above 90%, mailboxes created here in the last 30 days stop
  writing, so established vaults keep the last 10%; an attacker must prepare its
  mailboxes a month ahead. Age-based allowances were rejected: they accrue on idle
  attacker mailboxes and a reseed of a self-signed junk chain skips them. A first cut
  froze large mailboxes instead, which freezes the busiest honest vaults first. The
  hosted delivery cap drops to 64 MiB (a 64-member key generation leaves ~36 MB until it
  is done); the log stays 512 MiB, sized for honest lifetimes, not strangers.
- **NEW 9, entry size** (second review): `MAX_ENTRY_BYTES` equalled the body cap, so an
  entry as large as an append takes couldn't fit a reseed body (which also carries the
  member list): every later restore and move got 413 for good. Stored items are now at
  most `MAX_BODY_BYTES - 64 KiB` (appends and envelopes).
- **Errors in the app**: a full log ("This vault's history is full on this server. Move
  the vault to a server with more room.") and a full server have their own messages
  (`RelayLogFull`, `RelayFull`); the rest keep "the server's storage for this vault".
- **Pushes**: per device token (one FID serves all of a phone's vaults), at most one per
  `PUSH_WINDOW_SECS` (30 s) plus one trailing push at the window's end when any were held
  back (never dropped; the binary flushes every 5 s). FCM sends go through a token-deduped
  queue with at most 32 in flight. A token is forgotten on 404 `UNREGISTERED`, 403
  `SENDER_ID_MISMATCH`, and 400 `INVALID_ARGUMENT` only when its `google.rpc.BadRequest`
  names `message.token`/`message.fid` (FCM: "INVALID_ARGUMENT can also be returned in
  cases of issues in the message payload").
- **Charges**: a reseed already charged only its request's signer. The real case was
  `/v1/seat/cancel`, which takes no request signature and charged the old key on every
  post, so the thief whose move was kept could replay the shown cancel and rate-limit the
  owner. The cancel is verified before the lock and charged to the old key only when it
  moves a row from pending to cancelled; reposts cost the old key nothing.
- **Entry 0**: when the log is empty, author, poster and creator must be the same key
  (a joiner could take index 0 and strand the setup, which then couldn't be cancelled).
  Restores and relay moves write entry 0 through reseed.
- **Redirects**: every relay client is built with `redirect::Policy::none()` (reqwest
  followed up to 10, resending the body on 307/308 and the create token across hosts);
  the Tor path follows none.
- **Caddy** takes `max_size 1MiB` in both site blocks (`1MB` was 1,000,000 bytes to
  go-humanize; the relay takes 1,048,576).
- `post_envelope` and `append_entry` verify the signature before the lock (the request's
  signer is the envelope's sender; an entry's author key is in its header) and keep only
  the membership checks under it.
- No `RELAY_API` bump (no body changed; 507 bodies are now exactly the quota's text, and
  `/v1/info` limits gained fields); `RELAY_DB` 7 (`deliveries.sender`, `inbox_counts`,
  `log_counts`, `deliveries_by_age`; the migration reads each stored envelope's sender and
  each entry's author).
- Dropped: unknown and foreign mailboxes answering the same (see Residuals).

### Privacy

- Message kinds that reveal a move travel inside the sealed payload; votes are padded to
  fixed sizes so entry lengths don't show who approved.

## Residuals (accepted, with reasons)

- A keep counts for the seat's lifetime: round 1 can't be kept, so it rests on the
  approvers' warning ("call them first") plus any one helper's **Stop helping**.
- In a 2-of-3 a stop freezes that seat for good, since no other helper exists (as for a
  helper that never returns). The other two can still sign and move the funds to a new
  vault.
- The owner is assumed to open the app within the 72 h after the push; a relay that
  withholds the push and the keep (with a dishonest member and a fooled co-signer) still
  takes the seat (below).

- A seat can still be taken by a dishonest member, a fooled co-signer and a colluding
  relay together: the relay can withhold the old phone's push and its `KeepSeat`. The
  owner then sees only a fork (`RelayForked`) afterwards.
- During a version transition, shipped older apps keep acting on the old rules. For key
  repair that is a stall, never a leak: messages are keyed by (index, attempt, sender) so
  attempts never mix, a re-run of part 1 is a fresh additive sharing, and a dishonest
  helper named in a retry learns nothing RTS doesn't already give it. A v10 app ignores
  v11-tagged entries entirely, so an older helper bumps `attempt` only on tag-10
  `RepairRetry` entries (from other old apps), never on v11 votes, and stops matching the
  others until it updates: still a stall. From v11 only current
  members' entries raise the version floor or count as newer, so a former member's phone
  can't change the rules or raise "update Zafe"; a current member's fake future tag only
  shows "update Zafe". An older app keeps signing one-tap under the old assignment: a
  stall, never a nonce reuse (each nonce is deleted before its share is sent), so any
  change to one-tap assignment must be gated on every member having updated.

- Up to 3 tiny change notes or extra real vault notes can still raise a fee by 5,000 zat
  each; the fee is shown at approval (fee rule notes in `verify.rs`).
- At least two colluding members can still reserve notes, but only for the expiry window
  and only until rejected.
- A W logged after a proposal N doesn't make N respend its notes, so a later W that
  duplicates an earlier N can mine alongside it; t members approved both.
- lightwalletd supplies the tip and spends a member checks a cancelled payment against: a
  lying server can make one look dead (expired, or its notes spent deep) to a member using
  it. Every approving member's server must be fooled.
- "Dropped" is read from the member's own lightwalletd: a broadcast seconds old that hasn't
  reached it yet reads as dropped. An approver then refuses a payment that doesn't respend
  it (a retry passes), or a proposer spends its notes in a transaction that conflicts with
  the mempool copy: a lost round, never funds.
- A member that re-approved after a signing round (fresh commitments) and released a share
  in the earlier round can still reject: the earlier reply leaves no trace once sent.
  Possibly signed already covers it (the proposal reached Approved for that round).
- Proposal anchors are not checked: the wallet keeps 100 checkpoints (zcash_client_sqlite
  `PRUNING_DEPTH`) while proposals live for days, so a member can't tell an old anchor from
  a made-up one. A bad anchor only makes the transaction unmineable (DoS).
- After a cancelled payment's funds are moved (or another payment spending them is
  sent), new payments wait until that spend is 10 blocks (scaled) deep, about 12 minutes:
  until then a reorg could bring the cancelled one back, and `propose` says "try again in a
  few minutes".
- Zero-value dummy outputs' ciphertexts are random by construction: a covert channel from
  the proposer remains there (not in change or payments).
- `zafe interop` co-signs frost-zec PCZTs with the same checks, so change must carry the
  internal OVK (the interop test passes: zafe builds those PCZTs).
- lightwalletd is trusted for chain data (tip, notes, mempool). A lying server can delay or
  hide, not make members sign a different payment. Users can choose their server and Tor.
- The relay can link one phone's vaults through its push installation id. Push is per app
  install; turning push off for a vault avoids it.
- An invite is a bearer credential until the setup is sealed (design, spec §5).
- Unknown and foreign mailboxes still answer differently (404 vs 403), so whoever holds a
  label (an old invite, a former member, a move link) can learn whether that vault is on
  a relay. Only label holders can probe; create and reseed on an existing label must refuse
  anyway, so they would stay an oracle (at one creation token per probe); and answering a
  removed member 404 would show it "the server lost your vault" instead of a refusal.
- A moved seat's old share still works (spec §10.1 "Limits").
- **A public relay can still be filled by a stranger** with enough mailboxes made a month
  ahead (one IP makes 20 a day), and then every vault on it stops writing until the
  operator deletes them; quotas only set how many it takes. The real fix is admission
  control on hosted relays (a create token, or proof of work in create and reseed: a body
  change) plus an operator alert on database size. The hosted 2 GiB per network comes
  from 14 daily backups on the same disk; fewer local copies, or compressed or off-site
  ones, would allow a larger cap. Operator calls, not made here.
- **The log is a lifetime limit per relay**: 512 MiB hosted is about seven years at 10
  payments a day, one author's half about three and a half; a move copies the log, so
  only a relay with a larger cap helps (the app says so).
- Two colluding members can fill a vault's log or its undelivered messages (each may use
  half); the threat model is one dishonest member.

## Order

1. Safety words, recovery and address.
2. Seat moves (findings A, B, E): first, because one dishonest member can take a seat.
3. Proposals.
4. Relay server.
5. Relay moves.
6. Privacy.

Each batch: a failing test first for every finding, then the change, then the full local
CI and the live end-to-end scripts; reviewed and attacked again before it lands.

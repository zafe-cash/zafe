# Relays: choosing one and moving a vault

Why a group would leave its relay, the least-friction way to do it, the design every case
leads to, and a part-by-part security and privacy review (§8). Designed 2026-10-25; built
2026-10-25 to 2026-10-27 on branch `relay-client` (what was built: §7; what wasn't: §12).
As of 2026-10-27 none of it is merged or deployed: the hosted relays answer 404 on
`GET /v1/info`, and the relay must be promoted before an app from this branch ships
(`docs/tracker.md` "Now").

It reopens part of the 2026-10-06 decision "one relay per vault, no relay migration"
(`docs/tracker.md` "Decisions"). Principles come from `docs/onboarding.md` §5.1. This
document says **relay**; the app calls it the **server**.

**Goal: no lock-in, including from us.** A group can run its own relay, use a community relay,
or leave ours at any time, without losing funds or history, and a relay that died is not a
special case. **No relay is trusted, and none is needed:** a relay is a pipe and a cache;
everything a phone relies on is checked against member signatures, its own copy of the
history, or values passed between members sealed or out of band.

## 1. Where we started (2026-10-25)

Every point here except push changed with this design (§7).

- **Funds never depend on the relay.** Each phone holds its key share, the vault keys and its
  wallet, and syncs from lightwalletd. Balance and receive work without a relay.
- **Without a relay, nothing can be proposed, approved or sent.**
- **Each phone keeps a verified copy of the vault log** (`log_cache`). `node::reseed_relay`
  rebuilds a vault on any relay from one copy; the app offered it only on the same relay.
  Backups didn't carry the copy, so a restore trusted its first load.
- **The relay URL was one app-wide setting per network.** Invites didn't carry it.
- **Log entries and envelopes are bound to the mailbox id** (`EntryHeader.mailbox`, in the AAD
  and signature; `Header.mailbox` for envelopes), and the relay filed a vault under that same
  id (`reseed` and `log_append` in `crates/zafe-relay/src/lib.rs`).
- **A relay never deleted a mailbox** (`Relay::prune` deletes only old deliveries).
- **The unapproved-spend alert stopped during an outage** (`api/spends.rs` loaded from the relay).
- **Push works only from our relay** (FCM with Zafe's service account) (§10).

A vault lives on exactly one relay at a time: the relay orders the log. Zcash servers are a
per-phone choice because they all serve the same chain; a relay is a per-vault one.

## 2. Why a user would want this

| # | Situation | What they really want | Group move? |
|---|---|---|---|
| 1 | Our relay is down for hours | Their payment to go out | No: wait (status shows since when); a standby relay would make it rare (§12) |
| 2 | One member can't reach the relay (blocked country, work network) | That member connected | No: Tor, per member |
| 3 | A member doesn't want the relay to see their IP | Privacy | No: Tor, per member |
| 4 | The relay lost its data | The vault back | No: "Restore vault on the server" |
| 5 | A DAO evaluating Zafe asks "can we leave you?" | A credible yes before adopting | No: relay choice at creation, a self-host guide, and §4 documented |
| 6 | A DAO wants its own relay from day one | Self-hosting | No: choose it at creation |
| 7 | **Our relay is gone for good** | Keep using the vault | **Yes**, old relay dead |
| 8 | **They decide later to self-host**, or their community relay is closing | Move when it suits them | **Yes**, old relay alive |
| 9 | **They distrust their relay** (the app flags a different history or other members) | Get off it | **Yes**, old relay hostile |
| 10 | The relay is slow or flaky | A better one | **Yes**, rarely |

Six of ten need no move. Moves are rare, so a simple flow that always works beats a clever
automatic one.

## 3. Where every case converges

Each move needs three things:
1. **One member picks the destination.**
2. **Everyone learns where it is through a channel that works even when the old relay doesn't:**
   the group's own chat, already used for the invite and the safety words. A move works like
   an invite: a link.
3. **Enough members on the new relay that the ones left behind can't pay without them.**

**The rule people see: "Grants switches when 3 of 4 have moved."** The number is
`m = max(t, n − t + 1)`: at least t (so payments work there) and more than the members who
could be left behind (so they can never reach t on the old relay). For 2-of-3 and 3-of-5 that
is t; for 2-of-4 it is 3, for 2-of-5 it is 4. Until m members have moved, the vault stays
where it is.

Dropped, and why (each pass was critiqued before the next):
- **Anything that relies on the old relay** (votes there, redirects): fails when it's dead or
  hostile.
- **Anything that needs planning ahead** (fallback relays, your own domain): fails the groups
  that didn't plan; the DNS variant also pushes infrastructure work onto users.
- **Auto-follow without a tap:** one member could move everyone to a relay they run.
- **A vote shown as an approval:** a server setting dressed up as a money decision. The tap to
  follow is the consent.
- **Two flows by relay state:** people do the same thing either way; a working old relay only
  adds a second way to *receive* the move (the Needs you card, §4.3).

## 4. The design

Built as described here. The screens changed after research on 2026-10-27 (one Server
screen per vault, moves as Needs you cards, "server" in the UI); this section describes
what shipped on the branch.

### 4.1 Choosing at creation

Create > More options > **Server**, defaulting to ours. The invite carries the URL and the
relay's mailbox id (`INVITE` 3); the join screen shows the relay as a fact:
`Grants · 2 of 3 · relay.dao.org`. A new vault's mailbox id is random (§4.6), never the vault id.

The server list (`relay_picker.dart`, presets in `core/config/relay_presets.dart`) shows our
relays (one per network today) and any community relays (added by pull request, each with an
operator name; none yet), then **Other address** (type, paste or scan; `host#token` fills in a
create token) and **Run your own server →** (<https://zafe.cash/docs/run-a-server>). Only the
relay the user selects is checked (health, version, limits, "No notifications" when true),
through Tor when Tor is on. The list itself is not probed in the background (§8, box B).

Settings keeps the two servers apart: **<vault> > Server** (everyone in the vault) and
**This phone > Zcash server**. There is no app-wide relay row: a vault from before per-vault
relays gets the app-wide URL saved into its material once.

### 4.2 Moving

Settings > Grants > **Server** (`/vault-server`, `vault_server_screen.dart`) is the one place
for the vault's relay. While a move runs it becomes the move's waiting room.

```
 Server                               Server, after Move
 ┌──────────────────────────────┐    ┌──────────────────────────────┐
 │ Address   relay.zafe.cash    │    │ Moving Grants to             │
 │ Run by    Zafe               │    │ relay.dao.org                │
 │ Status    Connected          │    │  ✓ You          Moved        │
 │ Notifications  On            │    │  · Bob          [ Nudge ]    │
 │                              │    │  · Carol        [ Nudge ]    │
 │ Everyone in Grants uses this │    │  · Dan          [ Nudge ]    │
 │ server to work on payments   │    │ Grants switches when 3 of 4  │
 │ together. It only sees       │    │ have moved.                  │
 │ encrypted messages.          │    │ They see the move in Zafe.   │
 │                              │    │ Nudge sends one of them the  │
 │ [ Move to another server ]   │    │ link.                        │
 └──────────────────────────────┘    │   Cancel move                │
                                     └──────────────────────────────┘
```

- **Move to another server** opens the server list ("Move to"; the current relay is shown,
  never chosen), then the unlock. It is off, with the reason, while a member's app is too old
  ("Bob must update Zafe first.", §4.7) or two moves were found.
- `node::move_vault` picks a random mailbox label and move id, creates the mailbox on the new
  relay from this phone's history (`reseed_relay`, with the create token for a private relay),
  sends a sealed **arrival** (§4.4) to every member there, and leaves a sealed **move notice**
  for every member on the old relay if it answers.
- **Members learn of the move in the app**, as a Needs you card (§4.3), whenever the old relay
  delivers the notice. **Nudge** shares the link with one member: "I'm moving our Zafe vault
  Grants to relay.dao.org. Open this to move too: https://zafe.cash/move#…". The fragment holds
  the vault id, relay URL, label and move id, never the create token; the site never sees it.
  The link is the way in when the old relay is dead. (The design first opened the share sheet
  for everyone at Move; the 2026-10-27 rework replaced it with the card and per-member Nudge.)
- While the move runs, Home shows "Moving Grants to relay.dao.org · 1 of 4 moved", which opens
  the waiting room. A member who followed sees the same room, without Cancel.
- After the switch the mover sees "Grants moved to relay.dao.org" and Done.
- **The mover's phone keeps using the current relay until the switch.** Payments in progress
  carry on.

### 4.3 Following

```
 Bob, Home > Needs you                  Bob, the card opened (or the link)
 ┌──────────────────────────────┐      ┌──────────────────────────────┐
 │ Move Grants to relay.dao.org │      │ Move Grants to relay.dao.org?│
 │ Alice asked · 1 of 4 moved · │      │ Alice is moving it. 1 of 4   │
 │ your funds don't move        │      │ have moved.                  │
 │ [ Move ]  Not now            │      │ Your funds don't move.       │
 └──────────────────────────────┘      │ [ Move ]   (unlock)          │
                                       │   Not now                    │
                                       └──────────────────────────────┘
```

- **The card** (`needs_you.dart`) comes from the move notice on the current relay. Move
  follows in place (after the unlock); Not now hides it. Tapping the card opens the follow
  screen (`/vault-server/follow`, `relay_move_screen.dart`), which the link opens too.
- **Nothing connects to the new relay before Bob taps Move.** "Alice is moving it" and the count
  come from the move notice (if the old relay delivered it); a link alone shows "From a link".
  Then the phone connects (`node::follow_move`), and **refuses** unless:
  1. the new relay holds a valid arrival for this move id **signed by a current member** (so a
     link alone, or one forged by a hostile relay, is never enough);
  2. the move isn't expired or older than this phone's last switch;
  3. the relay's history passes the checks every load does (`RelayRolledBack`/`RelayForked`/
     `RelayMembership`).
  It then adds any entries its copy has that the relay lacks and sends its own arrival.
- **Lines shown only when true:** "No notifications on this server"; "relay.dao.org will see
  your IP address. Tor hides it." for a relay not on the built-in list.
- **After the switch**, a phone that hasn't followed shows "Grants moved to relay.dao.org" /
  "Move to continue", with no Not now.
- **One move at a time per phone:** a phone that followed one move can't follow another until
  that one switches, expires or is cancelled.

### 4.4 Arrivals and the switch

- **An arrival** is a signature by the member's vault key over `(vault id, move id, relay origin,
  mailbox id, base, expiry)`, where `base` is the index and hash of the last log entry the
  mover's copy held. It is sent sealed, one per recipient (`Recipient::All` envelopes are
  plaintext), so the old relay never sees where the group is going. Moves expire after 14 days.
- **A move notice** (on the old relay) carries the same fields plus the mover's arrival, sealed
  per recipient. It's never acknowledged, so a phone that was off finds it until the relay's
  30-day delivery prune.
- **The switch is one log entry:** the first phone that holds m valid arrivals for a move
  catches the new relay up (from the old relay if it answers, then from its copy) and appends
  `VaultMoved { move id, relay origin, mailbox id, base, arrivals }` there. Replay accepts it
  only with m distinct current members' unexpired arrivals for that move. It's written by the
  app, never shown as an approval.
- **Every phone switches when it sees `VaultMoved`:** those that followed watch the new relay;
  the rest learn from the move notice on the old relay and check `VaultMoved` on the new one
  before switching. Switching re-registers push on the new relay and unregisters on the old
  one (signed `/v1/push/forget`). Activity: "Grants moved to relay.dao.org".

### 4.5 Rules that keep the history whole

1. **No new entry on the new relay before `VaultMoved`.** `VaultMoved` itself is the first one.
   Copying entries that already exist (catch-up) is allowed. Payments need t anyway, so nothing
   waits that could have gone ahead.
2. **A phone that has sent an arrival checks the new relay for `VaultMoved` before every write to
   the old relay.** Once it exists, the phone writes only to the new relay and re-sends what it
   was about to write there as the same bytes (PCZT, votes, kept one-tap votes,
   `PoolStore::keep_vote`).
3. **A phone that sees `VaultMoved`** (directly, or through a move notice it then verifies) stops
   writing to the old relay and shows "Grants moved to relay.dao.org. Move to continue."
   Every write to the vault's current relay reads its move notices first: a phone that was
   off during the switch learns of it only there, and its app writes on its first refresh
   (one-tap commitments, its version) before any screen shows the notice.
4. **Everything carries over:** open payments, votes, one-tap commitments and shares, and note
   reservations are in the log; the leader re-sends a persisted interactive request (`<id>.req`).
5. **Old relay dead, fewer than m moved:** nobody can pay; the payment screen and the waiting
   room say "Payments resume when 1 more member moves to relay.dao.org."

Why this is safe: at most `n − m < t` members are left behind, so the old relay can never
complete a payment after the switch. Members who sent an arrival check before each write, so the
only window is a write that races `VaultMoved` by milliseconds; it ends up in the existing fork
flow and can't complete a payment on its own. (That flow has no screen after a move yet: §12.)

### 4.6 Labelled mailboxes (the anti-squatting change)

Before this change a vault's mailbox id was its vault id, and that id is in every invite.
Anyone who knew it could create that mailbox on other relays first, filled with a fake log the
blind relay can't tell apart, so the group could never move there. A hostile current relay, us
included, could do it to every listed relay.

- **A relay's mailbox id is a random label**, separate from the vault id the entries carry.
  New vaults and every move get a fresh one. The mover creates the mailbox **before** anyone
  else learns the label, which travels only sealed or in the link's fragment.
- **The relay stores the vault id from the first entry** and requires it on every later one, in
  place of "the entry's mailbox is the mailbox". `log_append` finds the mailbox from the entry
  header, so labelled mailboxes have their own routes that take the label (no `RELAY_API` bump;
  a 404 means an older relay, shown as "Needs an update" in the server list).
- **Envelopes and signed requests use the label**, so nothing from one relay verifies on another.
- Existing vaults keep their current id where they are; their next move gets a label.

### 4.7 Edge cases

| Case | What happens |
|---|---|
| Alice cancels before the switch | A sealed "move cancelled" voids her arrival. Cancel can't cause a split: the switch needs `VaultMoved` with m arrivals in the log, and if a stale arrival is used anyway, everyone acts on that one entry |
| Two members move to different relays | m > n/2, and a phone follows one move at a time, so only one can reach m. The other expires |
| A member is on an older app | **Move** is offered only once every current member's app wrote a version 10 event (`VAULT_EVENT` gate); the Server screen names who must update |
| A phone was off during the move | It reads the move notice on the old relay, verifies `VaultMoved` on the new one and switches. After the 30-day prune, or if the old relay is dead, only the link brings it over (a co-signer's Nudge) |
| A restore from a pre-move backup | Same as a phone that was off. The backup's log copy anchors the history (§6) |
| The old relay comes back | Moved phones ignore it; stragglers find the notice and switch |
| A member wants to stay | They can; they can't pay alone (n − m < t); their phone shows "Move to continue" |

## 5. During an outage

Home's status line turns orange: `Can't reach server`. It opens the Status screen
(`sync_status_screen.dart`): "Payments can't be approved until the server is back. Your funds
are safe.", with "Down since 09:14" and **Move to another server**. No notification.

After 6 hours down (`kSuggestMoveAfter`) the Status screen suggests the first listed relay
other than the current one that answers and can hold the vault: "Move Grants to …? Still down
after 6 hours. Your funds are safe, and payments go on once enough members move." with Move and
Pick another server. With one listed relay per network today, the suggestion needs a second
one first (§12).

The same Move button shows for every failure a move can fix: unreachable, rolled back, lost
vault, forked, wrong membership, outdated relay (`SyncFailure.canMoveRelay`).

| Keeps working | Stops |
|---|---|
| Balance, receive, history | Propose, approve, send |
| Unapproved-spend alert, from this phone's log copy ("Based on this phone's history from 09:14.") | Push, new members, seat recovery |

Our own outages should be invisible: a standby relay at a second provider, restored from the
Litestream replica, with a tested DNS switch and a scheduled restore drill. Not built (§12).

## 6. Relays own their data

Every phone holds the history, and so do backups, so a relay's copy is a cache. Users never
clean up after themselves on a relay:
- **Inactive mailboxes expire:** no authenticated member request for 1 year → deleted.
  Background checks keep active vaults alive. Last-seen is stored per day, not per request.
- **Unused copies expire sooner:** a mailbox where no member other than its creator has made a
  request within 30 days is deleted. A cancelled or failed move goes; a restored vault everyone
  uses stays, even if no one appends.
- **Backups carry the log copy** (`BACKUP` 3), so no relay is ever the only holder of a vault's
  history, even when every phone was lost.
- **Private relays:** `ZAFE_RELAY_CREATE_TOKEN`, when set, is needed to create or reseed a
  mailbox. Any member may need it later to restore, so it travels only sealed between members
  (in the arrival, in the log-key message at setup, in a recovered seat's sigma) and in
  backups, never in links or invites.

Operator settings and defaults: `infra/relay/README.md`.

## 7. What was built

Built in this order, each protection with or before what it protects: per-vault relay, labels
and anchors; relay routes, expiry, create token and picker; moves; the UI rework. Format
versions are recorded in `AGENTS.md` ("Bumping a version").

### 7.1 Relay (2026-10-25)

- **Labelled append:** `POST /v1/log/append-labelled`, a `Signed<LabelledAppend { mailbox,
  entry, timestamp }>` by any member of that mailbox (catch-up posts others' entries); the
  entry's author must be a current member, as on `/v1/log/append`. `RelayClient::append_log_to`.
- **Vault id:** stored from the first entry (append or reseed); any entry of another vault is
  400 "this mailbox holds another vault". Old mailboxes were migrated with `vault_id = id`.
- **Push forget:** never answers 404 (nothing registered is nothing to forget), so a 404 always
  means an older relay; it doesn't count as use for expiry.
- **`GET /v1/info`** is JSON (fields can be added without a `RELAY_API` bump): versions,
  limits (body, members, quotas, key rate, delivery retention), push, create token, expiry.
- **Expiry:** `last_seen_day`, `created_day` (day numbers) and `shared` (a member other than
  the creator made a request). The 30-day rule applies to every mailbox, not only labelled
  ones: every new mailbox is a label, and a setup nobody joined is as dead as a move nobody
  followed. Migrated mailboxes count as shared and seen on upgrade day, so only the 1-year
  rule applies to them. Off in `Relay::new()`, on in the binary (`ZAFE_RELAY_EXPIRY`,
  `ZAFE_RELAY_IDLE_DAYS`, `ZAFE_RELAY_UNUSED_DAYS`).
- **Create token:** a request header (`zafe-create-token`), so no body changed; refusal is
  401 with `www-authenticate: zafe-create-token` (`RelayClientError::CreateTokenRefused`).
  Stored as a BLAKE2b hash, compared with `subtle`. The per-IP creation bucket is charged
  before the check, so every guess costs one creation.
- **Members:** at most 64 per mailbox, now on `create` too (vaults have at most 15).
- **Older relays:** a 404 whose body isn't "unknown mailbox" → `RelayClientError::RelayTooOld
  { route }`; the bridge maps it to `RelayOutdated`.
- `RELAY_DB` 4. Tests: `crates/zafe-relay/tests/{labels,expiry}.rs`.

### 7.2 Client foundations (phase 2a, 2026-10-25: Rust core, CLI, bridge)

- **Where the per-vault relay lives:** in `VaultMaterial` (`VAULT_MATERIAL` 2, `relay:
  VaultRelay { url, label, create_token }`), not a separate file: it is in secure storage
  already, travels in backups with the material, and every core call that talks to the relay
  already has the material. `url: None` (all version 1 material) means the app-wide setting,
  so testers' vaults keep working; the bridge builds every vault client from the material
  (`vault_relay`) and treats the URL Dart passes as that fallback only. A move rewrites the
  material, as a seat move already does.
- **Two ids** (`node::VaultIds`): the vault id stays what keys, FROST identifiers, `sk`
  commitments, the safety words, the descriptor and every log entry are bound to; the label
  addresses the relay (envelope headers, signed requests, appends, inbox). The log copy is
  keyed by vault id, so it survives a move. `create_vault` picks both at random and refuses a
  relay without `/v1/info` (`RelayTooOld`): a new vault never falls back to a vault-id mailbox.
- **Invite** (`INVITE` 3): vault id, label, relay URL. A version 2 invite reads as label =
  vault id on the joiner's default relay.
- **Seat moves on a label needed one more relay route.** A `ReplaceApproval` is signed over
  the vault id (the same signatures are in the log), but `/v1/mailbox/replace` finds the
  mailbox by it. Added `/v1/mailbox/replace-labelled` (`LabelledReplace { mailbox,
  approval, approvals }`), which checks the vault id the label holds; the older route is used
  where label = vault id.
- **Create token:** the creator's client carries it (`with_create_token`) through create and
  every `setup_step`; it goes to the members sealed with the log key (`LOG_KEY_MESSAGE` 1;
  older creators' untagged 36-byte payload still reads) and to a recovered seat in the
  sigma; it is in backups through the material, never in invites or seat links. Restores
  (`reseed_relay`) send the vault's own token. `set_vault_create_token` lets a member add
  one later.
- **Backups** (`BACKUP` 3) carry the log copy. Export also pins the relay URL into the
  material it backs up (a legacy vault's app-wide relay), so the backup names the relay.
  Import verifies the copy against the material and installs it before anything talks to a
  relay (`node::install_log_copy`); a longer copy already on the phone is kept.
- **Seat recovery** (`REPAIR` 3): each sigma carries the helper's `LogAnchor { vault_id, len,
  head }` and the create token. The new phone checks every sigma's anchor right after reading
  the log, before using the log to decide who the helpers are (a relay that dropped the seat
  move itself is caught), and saves its copy only once everything checked out. The anchor
  also gives the vault id (the relay only knows the label); a version 2 sigma means a legacy
  vault (label = vault id). **Where recovery starts:** the new phone used to look only at the
  app-wide relay. A co-signer now gives it a **seat link** (`zafe-seat-v1:<hex>`, `SEAT_LINK`
  1: relay URL + label, not secret); the new phone runs the safety check and collects its key
  on that relay (`try_recover(.., Some(label))`). Without a link it checks every mailbox the
  default relay lists, as before.
- **Outage:** `node::state_from_copy` replays the phone's copy; the bridge's
  `unapproved_spends` falls back to it when the relay can't be reached (transport errors,
  502-504) and returns `from_copy_at`. Reachability per vault is `LogCache::relay_status`
  (`<vault id>.status`: last reached, unreachable since), written by `load_state` and the
  bridge's long-poll watch, at most once a minute while reachable.
- **CLI:** `--relay` always wins (one relay has different addresses from the host and an
  emulator); `--create-token`; `zafe seat link`; `zafe recover --link`.
- Tests: `crates/zafe-core/tests/{relay_rollback,relay_labels}.rs`.

### 7.3 Moves (phase 3, 2026-10-25: core, CLI, bridge)

Code: `crates/zafe-core/src/moves.rs` (formats, `MoveState`) and `node/relay_move.rs`
(`move_vault`, `follow_move`, `move_progress`, `cancel_move`, `move_notice`,
`announce_version`, `write_target`); bridge `app/rust/src/api/relay_move.rs`.

- **Formats.** `envelope::Kind::MoveNotice`, `Arrival`, `MoveCancelled` (appended). Their
  payload is `moves::MoveMessage` (`MOVE_MESSAGE` 1: `Arrival { target, signature,
  create_token }`, `Notice { target, mover, signature, create_token, moved }`, `Cancelled {
  move_id }`), length-prefixed and **padded to a multiple of 1 KiB**: a 2-of-3 signing
  request for one spend is ~0.7 KB, so a move message is one size bucket with the signing
  traffic rather than "the size of a vote" exactly (votes are log entries, not envelopes).
  `VaultEvent::VaultMoved { target, arrivals, at }` (`VAULT_EVENT` 10); `MoveTarget` is
  what every arrival signs (vault id, move id, normalized relay URL, label, base index +
  hash, expiry = start + 14 days). The link is `https://zafe.cash/move#<hex>` (`MOVE_LINK`
  1: vault id, relay URL, label, move id; never the token).
- **Replay** (`VaultState::check_move`): m distinct **current** members, every arrival
  valid (one bad or duplicate arrival voids the entry), the base entry's hash is the one at
  that index in this log, the move starts at or after the last switch, the move id is new,
  and the switching phone's clock `at` lies within the move's 14 days. Expiry can't be
  checked against a phone's own clock in replay (phones replaying later would disagree),
  so the clock check is at follow and switch time; `at` keeps replay deterministic.
  `VaultState::moves` lists every switch (Activity rows); the last is where the vault is.
- **The gate.** There was no "every member updated" mechanism before (earlier gates were
  "update together"). `VaultState::member_versions` records the highest event version
  each author wrote; Move is offered only when every current member wrote a version 10
  event. An updated app writes one once (`node::announce_version`: an idempotent `Name`
  re-stating its current name) on its first refresh (bridge `list_proposals`, CLI
  `proposals`, `pool`, keygen). The mover's Server screen names who must update, from the
  relay or, if it's down, from its copy.
- **Where a move lives on a phone:** `moves::MoveState` in `<vault id>.move` next to the
  log copy (`MOVE_STATE` 1, encrypted with a key derived from the vault secret, so the create
  token can be in it): target, mover, arrived, token, who got notices, cancelled,
  `moved_seen` (a verified `VaultMoved` seen), `switched` (verified on the new relay), and
  `from`, the relay it left. One open move per phone. The material follows a switch through
  `node::with_followed_relay` (the bridge applies it in `material()`, so every call uses the
  new relay at once; `list_proposals` returns `updated_material` for Dart to save, then push
  re-registers there). Because `from` is kept, starting the next move before the app saved
  the last switch never sends the phone back to an older relay.
- **Write rules** are in `append_event` (`relay_move::write_target`): rule 1 refuses an
  event on the new relay's label before the switch; rule 2 (a phone that sent its arrival)
  reads the new relay's log, verified on its own and compared with this phone's copy,
  before each write to the old relay, and **writes the same event bytes to the new relay
  itself** once `VaultMoved` is there (chain and state become the new relay's), rather
  than refusing and leaving the re-send to every caller: the bytes are never rebuilt and
  no caller can drop them. Before every write to the current relay it reads the move
  notices there (rule 3). `NodeError::VaultMoved { url }` is for a phone that saw the switch
  only in a notice and hasn't followed.
- **Follow** (`node::follow_move`): checks the new relay's whole log on its own (signatures,
  chain, membership incl. threshold) and against this phone's copy (`RelayForked` if they
  differ where both have entries), refuses `MoveNotFound` (no current member's arrival for
  that move id there, the mailbox missing, or the mover's cancel), `MoveExpired`,
  `MoveStale` (base before the last switch, or a move already superseded), `TwoMoves`
  (from the old relay's notices), then copies the entries the new relay lacks and sends its
  arrival. If the new relay already holds `VaultMoved` for the move, that entry (m signed
  arrivals) is the proof and the phone just switches (a phone that was off, a restore).
- **Counting and the switch** (`node::move_progress`, run on each refresh while this phone
  is in a move): arrivals are read from this member's inbox on the new relay; the mover's
  arrival carries the create token sealed. At m, `switch_relay` does a last catch-up from
  the old relay if it answers, catches the new relay up, appends `VaultMoved`, leaves it in
  move notices on the old relay (a `Notice` with `moved`), and forgets push there.
- **Notices** (`node::move_notice`, the Needs you card) read only the current relay. A
  notice's `VaultMoved` is verified offline against this phone's copy (author, signature, m
  valid arrivals, base where the copy reaches it); once it verifies, the phone stops writing
  to the old relay. A `MoveCancelled` from the mover hides the card.
- **Cancel** voids the sender's arrival (on the new relay) and, from the mover, the notice.
  Tested: with the cancel withheld from one phone, that phone can still switch with the
  stale arrival; everyone then acts on that one entry (the others see it in the notice).
- **Detection of a double move:** a verified `VaultMoved` notice with the same base as
  another one, or as a switch this vault already made, is `TwoMoves` (Home: "Two moves
  found"). A phone that already switched no longer reads the old relay, so detection is by
  stragglers and followers (§9 limit stands).
- **Relay compatibility:** relays decode envelope kinds, so a relay older than this build
  rejects move messages (400): notices to an old relay fail until it is updated (moves
  away from an old relay still work by the link). Promote the relay first.
- **CLI:** `zafe move <url> [--create-token]`, `zafe move --status`, `zafe move --cancel`,
  `zafe follow <link>`; `--relay-alias FROM=TO` (dev: one relay, two addresses). After a
  switch the CLI follows the vault's relay instead of `--relay`.

### 7.4 App (phase 2b 2026-10-25, rework 2026-10-27)

Server list, Create > More options > Server, the relay on the join screen, the per-vault
Server screen with its waiting room, the follow screen, Needs you cards, the Home move card,
the Status screen with "Down since" and the 6-hour suggestion, the payments-resume line, the
Activity row, seat links (share, QR, paste, scan). Screens are in §4 and §5; app notes in
`docs/app-notes.md`; previews in `app/tool/screens/relay_move_render_test.dart`.

## 8. Review: each part on its own

Threat model: relays are untrusted for integrity and privacy, and trusted only for
availability, which can be replaced. Members are trusted with the vault's privacy (they hold
`sk`) and with liveness, never with funds (t signatures). Network observers see what TLS and
Tor leave.

**A. Per-vault relay at creation.** Invites already are bearer credentials; adding the URL and
label adds no capability. A fake invite pointing at a hostile relay is caught by the safety
words, as before. ✓

**B. The picker.** *Found:* probing every listed relay for latency would tell every operator on
the list this phone's IP and that it runs Zafe, each time Settings opens. *Fixed:* only the
selected relay is checked, through `RelayClient`, so through Tor when it's on. A curated list is
not lock-in: any address can be entered. ✓

**C. The move (mover).** The new relay learns the members' keys, the threshold and the whole
encrypted history at once: entry count, authors, epochs, sizes, but no timing (it arrives in one
batch). That's less than the old relay learned over time. The mover lists the members;
`RelayMembership` catches a wrong list. ✓

**D. The link.** Holds the vault id, URL, label and move id, in the fragment. The chat app it's
shared in learns which relay the group uses and nothing it can act on: it isn't a member, so it
can't read, append or follow. *Found:* a link alone, or a hostile relay replaying an old move,
could steer a member. *Fixed:* following needs a current member's signed arrival on that relay,
and stale or expired moves are refused. ✓

**E. Follow.** *Found:* checking the new relay as soon as the card or link opened would reveal the
member's IP to a relay they never agreed to use. *Fixed:* nothing connects before the tap. ✓

**F. Counting and the switch.** *Found (two problems):*
1. **Split brain.** Switching at t lets both sides keep signing when t ≤ n/2. In a 2-of-5,
   2 members could move while 3 stay, and both groups could pay.
2. **Partial views.** Phones counting arrivals on their own could disagree, for example when a
   hostile relay withholds a cancel from some of them.

*Fixed:* the threshold is m = max(t, n − t + 1), and the switch is a single `VaultMoved` entry
carrying m arrivals that everyone reads from the same log. ✓

**G. Write rules.** *Found:* an arrived phone that hasn't noticed the switch could keep signing on the
old relay. *Fixed:* arrived phones check for `VaultMoved` before each write to the old relay. The
remaining race is milliseconds wide, ends in the fork flow, and can't complete a payment
(n − m < t). *Found in the emulator run (2026-10-26):* a phone that was off during the switch
forked the old relay with its first write. *Fixed:* every write reads the move notices first. ✓

**H. Cancel and competing moves.** m > n/2, one move per phone, and the switch is one log entry. So
among honest phones two moves can't both succeed, and a withheld cancel can't split the group. A
dishonest member signing two moves at once is covered in §9 (Members): it splits the history but
not the funds, and is detected through the old relay. ✓ (limit stated)

**I. Labelled mailboxes.** Squatting needs the label before the mover creates the mailbox, and
nobody but the mover has it then. Envelopes bound to the label can't be replayed across relays.
Former members hold `sk` but never receive a new label (it's sealed to current members). ✓

**J. Retention.** *Found:* expiring "reseeded mailboxes never appended to" would have deleted a quiet
restored vault after 30 days. *Fixed:* expiry counts member requests, not appends. Last-seen is
kept per day, to keep less metadata. A relay that deletes early only causes a restore. ✓

**K. Backups with the log copy.** The copy is encrypted with the log key, inside the
passphrase-encrypted backup; anyone who can open the backup already holds the keys. It anchors a
restore so no relay can serve a history shorter than the backup. A relay could still serve one
between the backup and the present: the worst case is a cancelled proposal whose signatures were
complete looking open again, which is already sendable by any member until it expires (spec
§9.5.2 consequences) and is invalidated by the sweep. No new power. ✓

**L. Recovered seats.** The helpers' sealed package carries the log head, so a new phone
anchors its first load. A relay can drop the package (delay), not change it. ✓

**M. Create token.** *Found:* keeping it only on the mover's phone meant other members couldn't
restore on a private relay. *Fixed:* it travels sealed between members and in backups, never in
links or invites. A member who leaks it lets strangers create mailboxes on that relay; the
operator rotates it. ✓

**N. Push.** Unregistering on the old relay tells it the group left, which it can see anyway. Push
itself only works from our relay (§10). ✓ (limit stated)

**O. Spend alert during an outage.** Runs from the phone's copy and says so, so it can't claim an
unapproved spend that a newer entry, not yet seen, would explain. ✓

**P. The relay itself (`crates/zafe-relay/src/lib.rs`, every route read against this design).**
- **Which routes find the mailbox how.**
  - Most routes take the mailbox from the signed payload: `inbox`, `inbox_ack`, `log_read`,
    `wait`, `register_push`, `mailbox_info`.
  - `post_envelope` takes it from the envelope header.
  - So labels work there unchanged, and an envelope from the old relay can't be posted to the
    new one: its header names the old label, and the lookup fails.
  - **Only `log_append` and reseed's entry check used the entry's vault id as the mailbox.**
    Those two changed: a labelled append route (`log_append_labelled`), and "the vault id
    matches the one stored from the first entry".
  - **Found in phase 2a: seat moves too.** `replace_member` takes the mailbox from the
    `ReplaceApproval`, which is signed over the vault id (the same signatures are in the
    log). Added `/v1/mailbox/replace-labelled`, which takes the label and checks the
    stored vault id.
- **Rule 1 is enforced by clients only.** Once a reseed finishes, the mailbox is open
  (`sealed = 1`) and the append routes accept any member's correctly chained entry. The
  relay can't tell catch-up from a new entry, so a dishonest member could write early and
  spoil a move for everyone (they'd hit `RelayForked`). That's a member delaying the group,
  inside the threat model; honest phones refuse the new relay and the group moves elsewhere.
- **The mover sets the relay's threshold, and the relay enforces seat moves with it.**
  `replace_in` checks approvals against the mailbox's threshold. A mover listing a lower t
  would let fewer members swap a seat *on that relay*. Covered: the membership check
  compares threshold as well as members (`check_relay_membership` → `mailbox_info`), and a
  mismatch refuses the relay before anyone follows.
- **There was no client route to drop a push token.** `forget_push_token` was only used by
  the relay itself, for tokens FCM rejects (`main.rs`). *Added:* a signed `/v1/push/forget`,
  so switching phones can unregister on the old relay.
- **Seat recovery assumed one relay.** A recovering phone found its vault with
  `/v1/mailboxes` on the relay it knew. *Added:* the seat link from a co-signer carries the
  vault's relay URL and label.
- **Anyone can reseed a new label listing other people's public keys as members.** That puts
  bogus entries in a victim's `/v1/mailboxes` list. It's bounded by `creates_per_ip`; clients
  ignore any mailbox whose log doesn't verify against what they hold.
- **Expiry covers every table.** `delete_mailbox` deletes the rows in `members`,
  `deliveries`, `sender_seqs`, `push_tokens` and `log_entries`, then the mailbox, and the
  quota counters go with the mailbox row. Last-seen is written at most once per mailbox per
  day, so reads don't add a write under the single DB mutex.
- **`/health` stays a plain "ok"** for load balancers. Version, limits and whether the relay
  sends notifications are in the unauthenticated `GET /v1/info`, which reveals nothing about
  mailboxes.
- **Size gives moves away.** The old relay sees a sealed message from the mover to each
  member, and a fixed-size message stands out. *Added:* move messages are padded to a
  multiple of 1 KiB, the bucket a small signing request falls in (§7.3).
- **Create token:** checked in `create` and in reseed's create branch, compared in constant time,
  never logged. A wrong token counts against `creates_per_ip`.

## 9. Review: the whole

- **Integrity.** Every fact a phone acts on is signed by members and checked against its own copy:
  entries, arrivals, `VaultMoved`, the relay's member list. No relay can forge, reorder into a
  different history, roll a phone back, or move a group.
- **Consistency.** The log stays one chain: no new entries on the new relay before `VaultMoved`,
  none on the old one after it from anyone who moved, and too few members left behind to pay.
  Every case that remains ends in the existing fork flow, never in two sides both able to pay.
- **Availability.** A relay can only stop serving. The group then moves, with no help from it
  (the link), and its history comes from any phone or backup. With every phone lost, backups
  rebuild the vault on any relay.
- **Privacy.**
  - Relays see what they saw before: keys, sizes, timing, IPs (unless Tor).
  - A new relay sees the history's shape at once.
  - The old relay never learns where the group went.
  - Nothing connects to a relay a member didn't choose.
  - Chat apps see a link they can't use.
  - What can't be removed: colluding old and new relays can recognise the same vault. The moved
    entries are byte-identical and carry the vault id, and members sign in with the same keys.
    Hiding that would mean re-signing the history under new identities. Tor still hides IPs.
- **Members.** A member can still delay the group, as before (never approving, never following).
  A dishonest member's app can sign arrivals for two moves at once; honest phones never do
  (one move per phone).
  - **Then two `VaultMoved` entries can exist,** one on each new relay, with the dishonest
    member counted on both sides (two sets of m > n/2 overlap only in members who signed
    both).
  - **Each side can pay only with that member's signature,** and every payment still needs its
    honest co-signers. The effect is the same as one member approving two different
    proposals, which is already possible: no theft, but a split history.
  - **Detection:** a switching phone also posts its `VaultMoved` into the move notice on the
    old relay. A phone that sees two moves for the same base refuses both and shows "Two
    moves found". With the old relay dead, it shows up when the two sides next exchange
    links. A phone that already switched no longer reads the old relay, so only stragglers
    and followers detect it.
  - A dishonest member still can't move funds alone: t signatures, unchanged.
- **Relays.** Never at users' mercy: they expire data on their own, can be private (create
  token), and advertise limits. Fake vaults stay bounded by the creation quotas.

## 10. Not now: push from any relay

Deferred (2026-10-25). A group on another relay gets no push; the server list, the Server
screen and the follow screen say so. For later: UnifiedPush lets any relay send standard Web
Push with its own VAPID key, which FCM accepts directly (Android, no Zafe key or gateway; no
iOS). https://unifiedpush.org/news/20250131_push_for_decentralized/

## 11. How we know it works

- **Moves** (`crates/zafe-core/tests/relay_move.rs`, 16 tests, relays A and B, 2-of-3 and
  2-of-5): moves with A alive and with A dead; 2-of-5 doesn't switch at 2 or 3 arrivals; a new
  entry on B before `VaultMoved` is refused; an arrived phone's write after `VaultMoved` goes
  to B; a restore from a pre-move backup follows; a cancelled move isn't followed; a cancel
  withheld from one phone still leaves one history; two moves; a link with no member arrival,
  an expired or a replayed move are refused; an envelope from A is dropped by B; a second move
  starts where the first went; a straggler can't fork the old relay; an older-app member
  blocks Move; no request to B before the tap.
- **Replay** (`crates/zafe-core/tests/vault.rs`): a `VaultMoved` with m − 1 or non-member
  arrivals is ignored; who must update.
- **Labels and anchors** (`crates/zafe-core/tests/relay_rollback.rs`, `relay_labels.rs`): a
  relay listing other members or another threshold is refused; a restored backup and a
  recovered seat refuse a relay with a shorter history; the spend alert falls back to the copy.
- **Relay** (`crates/zafe-relay/tests/labels.rs`, `expiry.rs`): one vault per mailbox whatever
  its label, entries with two vault ids refused, create token, wrong tokens charged, seat moves
  on a label, idle and unused expiry across every table, a quiet restored vault survives.
- **End to end:** `scripts/move-e2e.sh` (three CLI members move between two relays, a payment
  across the switch and one after it); the emulator run of 2026-10-26 (the app as member A,
  harness B and C, six relays: follow from the card and the link, the app as mover, payments
  across switches, old relay dead, cancel, restore onto the moved relay, seat recovery by
  link, create on a picked relay).
- **Not done:** a moderated session (a group moves to a relay they run; later we kill it
  mid-week and watch whether they find **Move to another server**).

## 12. Not built yet

Matches `docs/tracker.md` "Now: branch `relay-client`".

- **Ship order:** merge `relay-client`, promote the relay to mainnet, then release the app
  (relays decode envelope kinds, so an old relay rejects move messages). The site's
  `/docs/run-a-server` is built but deploys with the merge.
- **Leaving a fork after a move:** a write that races the switch forks the old relay, and
  `follow_move` then fails with `RelayForked`. No screen offers `node::follow_relay` for the
  new relay (the "existing fork flow" of §4.5).
- **The waiting room updates only on the next vault refresh** (up to a minute in the emulator
  run): refresh it when an arrival lands.
- **Site page `zafe.cash/move`** (opens the app with the fragment; explains a move to someone
  without Zafe), plus verified App Links and iOS Universal Links for `/move`. Today the link
  opens the app through an unverified intent filter.
- **Standby relay** at a second provider, restored from the Litestream replica, with a DNS
  switch and a restore drill (§5).
- **A second listed relay** (standby or community): with one per network, the Status screen's
  6-hour suggestion has nowhere to point. *(Not in the tracker yet.)*
- **Spec §6/§14 and `docs/audit-scope.md`** don't fully describe per-vault relays, labels and
  moves yet.
- **Push from any relay** (UnifiedPush): deferred, §10.
- **Known limit, not planned:** after a switch a phone stops reading the old relay's notices,
  so only stragglers and followers detect `TwoMoves` (§9).

## 13. Sources

- Delta Chat: "Adding or removing a relay will automatically inform your chat partners".
  https://delta.chat/en/help
- SimpleX: queue rotation "agreed between the clients", the new queue "not known to the
  server". https://discuss.privacyguides.net/t/simplex-vs-cwtch-who-is-right/19256/50.md
- Safe: with the service down, approvals "presented to the user for manual
  aggregation/import/export". https://hackmd.io/@safe/decentralize
- Nunchuk user: "We had to manually export/import transactions to get the funds out."
  https://stacker.news/items/398370
- Freenode seized 700+ channels that announced their move to Libera.Chat (May-June 2021):
  https://lwn.net/Articles/857252 , https://lists.gnu.org/r/info-gnu/2021-06/msg00007.html

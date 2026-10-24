# Relays: choosing one and moving a vault

Written 2026-10-25. Why a group would leave its relay, the least-friction way to do it, the
design every case leads to, and a part-by-part security and privacy review (§8). It reopens
part of the 2026-10-06 decision "one relay per vault, no relay migration" (`docs/tracker.md`).
Principles come from `docs/onboarding.md` §5.1.

**Goal: no lock-in, including from us.** A group can run its own relay, use a community relay,
or leave ours at any time, without losing funds or history, and a relay that died is not a
special case. **No relay is trusted, and none is needed:** a relay is a pipe and a cache;
everything a phone relies on is checked against member signatures, its own copy of the
history, or values passed between members sealed or out of band.

## 1. Where we are

- **Funds never depend on the relay.** Each phone holds its key share, the vault keys and its
  wallet, and syncs from lightwalletd. Balance and receive work without a relay.
- **Without a relay, nothing can be proposed, approved or sent.**
- **Each phone keeps a verified copy of the vault log** (`log_cache`). `node::reseed_relay`
  rebuilds a vault on any relay from one copy; the app offers it only on the same relay.
  Backups don't carry the copy, so a restore trusts its first load.
- **The relay URL is one app-wide setting per network.** Invites don't carry it.
- **Log entries and envelopes are bound to the mailbox id** (`EntryHeader.mailbox`, in the AAD
  and signature; `Header.mailbox` for envelopes), and the relay files a vault under that same
  id (`lib.rs:1055`, `lib.rs:1375`).
- **A relay never deletes a mailbox** (`Relay::prune` deletes only old deliveries; expiry
  is now built, see §7).
- **The unapproved-spend alert stops during an outage** (`api/spends.rs` loads from the relay).
- **Push works only from our relay** (FCM with Zafe's service account) (§10).

A vault lives on exactly one relay at a time: the relay orders the log. Zcash servers are a
per-phone choice because they all serve the same chain; a relay is a per-vault one.

## 2. Why a user would want this

| # | Situation | What they really want | Group move? |
|---|---|---|---|
| 1 | Our relay is down for hours | Their payment to go out | No: wait (status shows since when); our standby relay makes it rare |
| 2 | One member can't reach the relay (blocked country, work network) | That member connected | No: Tor, per member |
| 3 | A member doesn't want the relay to see their IP | Privacy | No: Tor, per member |
| 4 | The relay lost its data | The vault back | No: "Restore vault on the relay" |
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

**The rule people see: "Payments switch when 3 of 4 have moved."** The number is
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
  adds a second way to *receive* the link.

## 4. The design

### 4.1 Choosing at creation

Create > More options > **Relay**, defaulting to ours. The invite carries the URL and the
relay's mailbox id (`INVITE` bump); the join screen shows the relay as a fact:
`Grants · 2 of 3 · relay.dao.org`. A new vault's mailbox id is random (§4.6), never the vault id.

The picker reuses the Zcash server list: our relays plus community relays (added by pull
request, each with an operator name), then **Other address** / **Scan**, and **Run your own
relay →** (a site page: Compose, Caddy, backups, the create token). Only the relay the user
selects is checked (health, version, limits, "No notifications" when true), through Tor when
Tor is on. The list itself is not probed in the background (§8, box B).

Settings splits the two servers: **Zcash server · this phone** and **Relay · everyone in
Grants**.

### 4.2 Moving

```
 Alice                              Alice, after Move
 ┌──────────────────────────────┐   ┌──────────────────────────────┐
 │ Move Grants to another relay │   │ Moving to relay.dao.org      │
 │ [ picker ]                   │   │  ✓ You                       │
 │                              │   │  · Bob     not yet           │
 │ Everyone moves by opening a  │   │  · Carol   not yet           │
 │ link you send them.          │   │  · Dan     not yet           │
 │                              │   │                              │
 │ [ Move ]   (unlock)          │   │ Payments switch when 3 of 4  │
 └──────────────────────────────┘   │ have moved.                  │
                                    │ [ Share link ]               │
                                    │   Cancel move                │
                                    └──────────────────────────────┘
```

- **Move** (after unlock) picks a random mailbox id and a move id, creates the mailbox on the
  new relay from this phone's history (`reseed_relay`), and only then opens the share sheet:
  "I'm moving our Zafe vault Grants to relay.dao.org. Open this to move too:
  https://zafe.cash/move#…". The fragment holds the vault id, relay URL, mailbox id and move
  id; the site never sees it.
- Alice's phone sends a sealed **arrival** (§4.4) to every member on the new relay, and a sealed
  **move notice** to every member on the old relay while it answers.
- **Alice's phone keeps using the current relay until the switch.** Payments in progress carry on.

### 4.3 Following

```
 Bob: from the link, or a card on Home
 ┌──────────────────────────────┐
 │ Move Grants to relay.dao.org?│
 │ Alice is moving it. 1 of 4   │
 │ have moved.                  │
 │ Your funds don't move.       │
 │ [ Move ]   (unlock)          │
 │   Not now                    │
 └──────────────────────────────┘
```

- **The same screen** opens from the link or from a Home card when the move notice arrives
  through the old relay.
- **Nothing connects to the new relay before Bob taps Move.** "Alice is moving it" and the count
  come from the move notice (if the old relay delivered it) or are shown as "From a link" until
  the tap. Then the phone connects, and **refuses** unless:
  1. the new relay holds a valid arrival for this move id **signed by a current member** (so a
     link alone, or one forged by a hostile relay, is never enough);
  2. the move isn't expired or older than this phone's last switch;
  3. the relay's history passes the checks every load does (`RelayRolledBack`/`RelayForked`/
     `RelayMembership`).
  It then adds any entries its copy has that the relay lacks and sends its own arrival.
- **Lines shown only when true:** "No notifications on this relay"; "relay.dao.org will see your
  IP address. Tor hides it." for a relay not on the built-in list.
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
  one (new signed `/v1/push/forget`; the relay has no client route for it today). Activity:
  "Grants moved to relay.dao.org".

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
4. **Everything carries over:** open payments, votes, one-tap commitments and shares, and note
   reservations are in the log; the leader re-sends a persisted interactive request (`<id>.req`).
5. **Old relay dead, fewer than m moved:** nobody can pay; the payment screen says "Payments
   resume when 1 more member moves to relay.dao.org."

Why this is safe: at most `n − m < t` members are left behind, so the old relay can never
complete a payment after the switch. Members who sent an arrival check before each write, so the
only window is a write that races `VaultMoved` by milliseconds; it ends up in the existing fork
flow and can't complete a payment on its own.

### 4.6 Labelled mailboxes (the anti-squatting change)

Today a vault's mailbox id is its vault id, and that id is in every invite. Anyone who knows it
could create that mailbox on other relays first, filled with a fake log the blind relay can't
tell apart, so the group could never move there. A hostile current relay, us included, could
do it to every listed relay.

- **A relay's mailbox id becomes a random label**, separate from the vault id the entries carry.
  New vaults and every move get a fresh one. The mover creates the mailbox **before** anyone
  else learns the label, which travels only sealed or in the link's fragment.
- **The relay stores the vault id from the first entry** and requires it on every later one, in
  place of `h.mailbox == p.mailbox` (`lib.rs:1055`). `log_append` finds the mailbox from the
  entry header today (`lib.rs:1375`), so labelled mailboxes get new routes that take the label
  (no `RELAY_API` bump; a 404 means an older relay, greyed in the picker).
- **Envelopes and signed requests use the label**, so nothing from one relay verifies on another.
- Existing vaults keep their current id where they are; their next move gets a label.

### 4.7 Edge cases

| Case | What happens |
|---|---|
| Alice cancels before the switch | A sealed "move cancelled" voids her arrival. Cancel can't cause a split: the switch needs `VaultMoved` with m arrivals in the log, and if a stale arrival is used anyway, everyone acts on that one entry |
| Two members move to different relays | m > n/2, and a phone follows one move at a time, so only one can reach m. The other expires |
| A member is on an older app | **Move** is available only once every member's app supports it (`VAULT_EVENT` gate by tag); the screen names who must update |
| A phone was off during the move | It reads the move notice on the old relay, verifies `VaultMoved` on the new one and switches. After the 30-day prune, or if the old relay is dead: "Got a link from a co-signer?" |
| A restore from a pre-move backup | Same as a phone that was off. The backup's log copy anchors the history (§6) |
| The old relay comes back | Moved phones ignore it; stragglers find the notice and switch |
| A member wants to stay | They can; they can't pay alone (n − m < t); their phone shows "Move to continue" |

## 5. During an outage

The Home status reads `Can't reach relay · since 09:14`. The sheet: "Payments can't be
approved until the relay is back. Your funds are safe.", then **Move to another relay**. No
notification.

| Keeps working | Stops |
|---|---|
| Balance, receive, history | Propose, approve, send |
| Unapproved-spend alert (**fix**: from this phone's log copy, labelled "based on this phone's history from 09:14") | Push, new members, seat recovery |

Our own outages should be invisible: a standby relay at a second provider, restored from the
Litestream replica, with a tested DNS switch and a scheduled restore drill.

## 6. Relays own their data

Every phone holds the history, and backups will too, so a relay's copy is a cache. Users never
clean up after themselves on a relay:
- **Inactive mailboxes expire:** no authenticated member request for 1 year → deleted.
  Background checks keep active vaults alive. Last-seen is stored per day, not per request.
- **Unused copies expire sooner:** a labelled mailbox where no member other than its maker has
  made a request within 30 days is deleted. A cancelled or failed move goes; a restored vault
  everyone uses stays, even if no one appends.
- **Backups carry the log copy** (`BACKUP` bump), so no relay is ever the only holder of a
  vault's history, even when every phone was lost.
- **Private relays:** `ZAFE_RELAY_CREATE_TOKEN`, when set, is needed to create or reseed a mailbox.
  Any member may need it later to restore, so it travels only sealed between members (in the
  move notice, in a sealed message after joining) and in backups, never in links or invites.

## 7. What changes in code

| Area | Change |
|---|---|
| App storage | Per vault: relay URL, relay mailbox id, create token if any; Settings relay is the default for new vaults |
| Formats | `INVITE`: relay URL + mailbox id. `BACKUP`: URL, mailbox id, token, log copy. `REPAIR`: log length + head hash; the seat-recovery code carries the relay URL and label. Move notices and arrivals padded to a vote envelope's size. `envelope::Kind::MoveNotice`, `Arrival`, `MoveCancelled` appended, payloads versioned. `VaultEvent::VaultMoved` appended (`VAULT_EVENT` 10, gate). Record bumps in `AGENTS.md` |
| Core | `node::move_vault`, `node::follow_move`, arrival counting and verification, `VaultMoved` replay rule (m distinct current members, unexpired, one move), rules 1-3 of §4.5 in `append_event`, `NodeError::VaultMoved { url }`, notices never acknowledged, first loads anchored (backup copy, recovery head hash), spend alert from the log copy |
| Relay | Labelled mailboxes: a labelled `log_append` route and reseed checks the stored vault id (`lib.rs:1055`, `:1375`; the other routes already take the mailbox from the payload). `/v1/push/forget`. `GET /v1/info` (version, limits, notifications). Expiry across all tables (1 year idle; 30 days for unused labels; last-seen at most once a day). Create token. `RELAY_DB` bump (vault id, last-seen day) |
| App | Picker (selected relay checked only), Create > Relay, relay on join, Settings split, move / follow / list screens, `zafe.cash/move#` link, Home card, no connection before the tap, status "since" |
| Ops/docs | Standby relay + drill; built-in relay list; "Run your own relay"; spec §6/§14; audit scope §10 |

**Order:** (1) per-vault relay + labels for new vaults + invite/backup/recovery anchors;
(2) relay labelled routes, expiry, create token + picker + self-host page; (3) move and follow
with `VaultMoved`; (4) standby relay. Each protection ships with or before what it protects.

**Relay side built (2026-10-25, branch `relay-labels`, not deployed).** What it refines:
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
  ones: from now on every new mailbox is a label, and a setup nobody joined is as dead as a
  move nobody followed. Migrated mailboxes count as shared and seen on upgrade day, so only
  the 1-year rule applies to them. Off in `Relay::new()`, on in the binary
  (`ZAFE_RELAY_EXPIRY`, `ZAFE_RELAY_IDLE_DAYS`, `ZAFE_RELAY_UNUSED_DAYS`).
- **Create token:** a request header (`zafe-create-token`), so no body changed; refusal is
  401 with `www-authenticate: zafe-create-token` (`RelayClientError::CreateTokenRefused`).
  Stored as a BLAKE2b hash, compared with `subtle`. The per-IP creation bucket is charged
  before the check, so every guess costs one creation.
- **Members:** at most 64 per mailbox, now on `create` too (vaults have at most 15).
- **Older relays:** a 404 whose body isn't "unknown mailbox" → `RelayClientError::RelayTooOld
  { route }`; the bridge doesn't map it yet (phase 1 app work).

## 8. Review: each part on its own

Threat model: relays are untrusted for integrity and privacy, and trusted only for
availability, which can be replaced. Members are trusted with the vault's privacy (they hold
`sk`) and with liveness, never with funds (t signatures). Network observers see what TLS and
Tor leave.

**A. Per-vault relay at creation.** Invites already are bearer credentials; adding the URL and
label adds no capability. A fake invite pointing at a hostile relay is caught by the safety
words, as today. ✓

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
(n − m < t). ✓

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

**L. Recovered seats.** The helpers' sealed package now carries the log head, so a new phone
anchors its first load. A relay can drop the package (delay), not change it. ✓

**M. Create token.** *Found:* keeping it only on the mover's phone meant other members couldn't
restore on a private relay. *Fixed:* it travels sealed between members and in backups, never in
links or invites. A member who leaks it lets strangers create mailboxes on that relay; the
operator rotates it. ✓

**N. Push.** Unregistering on the old relay tells it the group left, which it can see anyway. Push
itself only works from our relay (§10). ✓ (limit stated)

**P. The relay itself (`zafe-relay/src/lib.rs`, every route read against this design).**
- **Which routes find the mailbox how.**
  - Most routes take the mailbox from the signed payload: `inbox`, `inbox_ack`, `log_read`,
    `wait`, `register_push`, `mailbox_info`.
  - `post_envelope` takes it from the envelope header.
  - So labels work there unchanged, and an envelope from the old relay can't be posted to the
    new one: its header names the old label, and the lookup fails (T7 holds by construction).
  - **Only `log_append` (`:1375`) and reseed's entry check (`:1055`) use the entry's vault id as
    the mailbox.** Those two change: a labelled append route, and "the vault id matches the one
    stored from the first entry".
- **Rule 1 is enforced by clients only.** Once a reseed finishes, the mailbox is open
  (`sealed = 1`, `:1083`) and `log_append` accepts any member's correctly chained entry. The
  relay can't tell catch-up from a new entry, so a dishonest member could write early and
  spoil a move for everyone (they'd hit `RelayForked`). That's a member delaying the group,
  inside the threat model; honest phones show "The new relay's history differs: ask Alice"
  and the group moves elsewhere.
- **The mover sets the relay's threshold, and the relay enforces seat moves with it.**
  `replace_member` checks approvals against `mb.threshold` (`:883-935`). A mover listing a
  lower t would let fewer members swap a seat *on that relay*. Covered: the membership check
  compares threshold as well as members (`check_relay_membership` → `mailbox_info`), and a
  mismatch refuses the relay before anyone follows.
- **There is no client route to drop a push token.** `forget_push_token` is only used by the
  relay itself, for tokens FCM rejects (`main.rs:71`). *Added:* a signed `/v1/push/forget`, so
  switching phones can unregister on the old relay (T10).
- **Seat recovery assumed one relay.** A recovering phone finds its vault with `/v1/mailboxes`
  on the relay it knows. *Added:* the recovery code from a co-signer carries the vault's relay
  URL and label.
- **Anyone can reseed a new label listing other people's public keys as members.** That puts
  bogus entries in a victim's `/v1/mailboxes` list. It's bounded by `creates_per_ip`; clients
  ignore any mailbox whose log doesn't verify against what they hold.
- **Expiry must cover every table.** It deletes the rows in `members`, `deliveries`,
  `sender_seqs`, `push_tokens` and `log_entries`, then the mailbox (the list `cancel_setup`
  uses, `:818`, plus `log_entries`), keeping the quota counters in step. Last-seen is written at
  most once per mailbox per day, so reads don't add a write under the single DB mutex.
- **`/health` stays a plain "ok"** for load balancers. Version, limits and whether the relay
  sends notifications go in a new unauthenticated `GET /v1/info` that reveals nothing about mailboxes.
- **Size gives moves away.** The old relay sees a sealed message from the mover to each member,
  and a fixed-size message stands out. *Added:* move notices and arrivals are padded to the size
  of an ordinary vote envelope.
- **Create token:** checked in `create` and in reseed's create branch, compared in constant time,
  never logged. A wrong token counts against `creates_per_ip`.

**O. Spend alert during an outage.** Runs from the phone's copy and says so, so it can't claim an
unapproved spend that a newer entry, not yet seen, would explain. ✓

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
- **Members.** A member can still delay the group, as today (never approving, never following).
  A dishonest member's app can sign arrivals for two moves at once; honest phones never do
  (one move per phone).
  - **Then two `VaultMoved` entries can exist,** one on each new relay, with the dishonest
    member counted on both sides (two sets of m > n/2 overlap only in members who signed
    both).
  - **Each side can pay only with that member's signature,** and every payment still needs its
    honest co-signers. The effect is the same as one member approving two different
    proposals, which is possible today: no theft, but a split history.
  - **Detection:** a switching phone also posts its `VaultMoved` into the move notice on the
    old relay. A phone that sees two moves for the same base refuses both and shows "Two
    moves found: ask your group". With the old relay dead, it shows up when the two sides
    next exchange links.
  - A dishonest member still can't move funds alone: t signatures, unchanged.
- **Relays.** Never at users' mercy: they expire data on their own, can be private (create
  token), and advertise limits. Fake vaults stay bounded by today's quotas.

## 10. Not now: push from any relay

Deferred (2026-10-25). A group on another relay gets no push; the picker and the follow screen
say so. For later: UnifiedPush lets any relay send standard Web Push with its own VAPID key,
which FCM accepts directly (Android, no Zafe key or gateway; no iOS).
https://unifiedpush.org/news/20250131_push_for_decentralized/

## 11. How we'll know it works

- **Harness:** relays A and B, members on 2-of-3 and 2-of-5. Moves with A alive and with A
  dead; payments continue on A until `VaultMoved`; a one-tap payment started on A completes on
  B; a phone off throughout follows later; a restore from a pre-move backup follows.
- **Rules:** a new entry on B before `VaultMoved` is refused; an arrived phone's write to A after
  `VaultMoved` goes to B; 2-of-5 doesn't switch at 2 or 3 arrivals; two moves; cancel with the
  cancel withheld from one phone; an older-app member blocks Move.
- **Attacks:** squat B under the vault id and every old id first (the move still works); a link
  with no member arrival (refused); a replayed old move (refused); an envelope from A replayed
  into B (dropped); a relay serving an old history to a restored phone and to a recovered seat
  (refused); a relay listing an extra member (`RelayMembership`); a `VaultMoved` with m − 1 or
  non-member arrivals (ignored).
- **Privacy:** no request to B before the tap; only the selected relay is checked; the old relay
  never receives B's URL in the clear.
- **Relay:** idle and unused-label expiry; a quiet restored vault survives; create token; a
  mailbox whose entries carry two vault ids is refused.
- **Moderated session:** a group moves to a relay they run; later we kill it mid-week and watch
  whether they find **Move to another relay**.

## 12. Sources

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

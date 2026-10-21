# Onboarding: research, process and design

Written 2026-10-15. Research, the study of Vizor's and Zodl's first-run flows, and a
proposed design for Zafe's onboarding. Nothing here is built yet except the error-copy fix
noted in §7. Track the work in `docs/tracker.md` ("Onboarding").

Sources used:
- A web research pass (NN/g, Apple HIG, Material, Monzo, Mercury, Ramp, Fordefi, Safe,
  Squads, Vultisig, Nunchuk, Casa, Bitkey, Zengo, and the usability papers cited below).
  Vendor numbers are self-reported and marked as such.
- A code study of Vizor (`~/zcash/vizor-wallet` at 15170d2, which has its own
  `docs/onboarding.md`) and of Zodl Android and iOS. Zodl is AGPL: ideas only.
- A read of Zafe's own flow (`features/onboarding`, `features/backup`, `app.dart`) and the
  security audit of 2026-10-14, which found two setup-time weaknesses (§5.4).

## 1. How this was done, and how the process changed

I ran the process three times, critiquing each pass before the next.

**Pass 1: a creator's funnel.** Map Welcome → Create → Invite → Keys → Home, find the
drop-off points, fix the screens. That is how most wallet onboarding is designed (Vizor's
doc follows it).

Critique: it designs for the wrong person. In an n-member vault, n−1 of the first-time
users are **joiners**. They arrive from a link sent by someone they know, often before
they have the app, and many are less invested than the creator. The funnel also assumes
one sitting. Zafe's setup is a ceremony that needs several people at once, then a week of
"first times" (first deposit, first approval, first lost phone). And it ignores that we
can't measure a funnel: no telemetry, ever.

**Pass 2: the vault as the unit.** Onboard a *group*, not a user. Three timelines:
1. The synchronous ceremony: join, compare, make keys. About ten minutes with everyone
   reachable.
2. The asynchronous waiting around it: invites sent but not opened, a member who never
   taps "Create vault keys".
3. The first week: backup, first deposit, first proposal, first approval, first
   notification.

For each moment I asked three questions. What can go wrong with *funds*? What can leave
people *stuck*? What does this person need to *understand* right now, and no earlier?

Critique: pass 2 produced a long wish list (practice vault, tours, checklists, tips,
presets) with no way to choose between items. NN/g's controlled study says
up-front tutorials don't help and make apps feel harder. Several ideas on the list were
exactly that. It also treated the safety number as a UX problem. It is a protocol problem
first: the audit found a malicious relay can grind the current 12-digit number (§5.4).
No amount of copy fixes that.

**Pass 3: ranked by harm, built on evidence, with protocol and UI designed together.**
- Rank every item by what it prevents: lost funds > a stuck group > a confused member >
  polish.
- Prefer patterns with evidence (cited below). Mark the rest as hypotheses to test in
  moderated sessions.
- Where the UI is only as strong as the protocol under it (safety number, recovery code),
  change both together.
- Plan the research we *can* do without telemetry (§8).

The rest of this doc is the pass 3 result.

## 2. Who is onboarding

| Person | Arrives | Wants | Worst outcome |
|---|---|---|---|
| Creator (committee lead, treasurer, one half of a couple) | App store or site | A working vault today, and co-signers who don't need hand-holding | Group stuck mid-setup; funds sent to a vault nobody can spend from |
| Joiner (committee member, partner) | An invite link in chat, often before installing | To do their part quickly and understand what they agreed to | Joins an impostor's vault; never backs up; misses approval requests |
| Returning member (new phone, reinstall) | App store | Their seat back | Can't tell "restore" from "recover"; loses the seat |
| Evaluator (DAO ops person checking Zafe) | Site, GitHub | Proof it works before real money | Has to gather three people just to try it |

The evaluator is the case for a practice mode (§6, P2). Today the only way to see Zafe
work is to recruit co-signers.

## 3. What the research says (the parts that change decisions)

1. **Up-front tutorials don't help.** NN/g, 70 users: task success 91% with a tutorial vs
   94% without, and perceived ease was *worse* with it (4.92 vs 5.49, p=0.047).
   https://www.nngroup.com/articles/mobile-tutorials/ Teach in context instead
   (https://www.nngroup.com/articles/onboarding-tutorials/), and teach by letting people
   "safely test an action" (Apple HIG, Onboarding).
2. **People don't find verification ceremonies on their own, but do them when told why.**
   Vaziripour et al. (SOUPS 2017): 14% completed key verification unprompted. After one
   explanation of *why*, 78%, and 96% in Viber, which had an in-app call made for reading
   the key aloud. https://zappala.byu.edu/pubs/alice-soups-2017.pdf
3. **How the code is shown matters more than its length.** Tan et al. (CHI 2017, n=661):
   attacks slipped past 6% of people comparing *words* and 21% comparing hex.
   "Pick the matching one" was the worst design (72% missed).
   https://users.ece.cmu.edu/~lbauer/papers/2017/chi2017-fingerprints-author.pdf
   Their advice for high stakes is to take the human out: scan, don't read.
4. **Backups fail silently unless they're rehearsed.** Mai et al. (SOUPS 2020) recommend
   asking for the secret back, and again later. Signal's PIN reminders (12 h, 1 d, 3 d,
   7 d, then every 14 d): 76% check when prompted, and only 12% report trouble
   remembering (Bailey et al., SOUPS 2021). Spaced practice works for secrets
   (Bonneau & Schechter 2014: 94% learned a 56-bit secret).
5. **Ask for backup when it starts to matter, and never let the reminder lose its slot.**
   Zodl shows the backup banner once the wallet has received funds. Its iOS code ranks
   that banner above everything, even "disconnected", after "three users lost funds while
   this rung sat at 6" (`SmartBannerStore.swift`, MOB-1786). Bitkey won't let you use
   the wallet until its cloud backup succeeds.
6. **Check every key before funding, then test with a small amount.** Casa: "Before
   depositing any funds, perform a health check on every key." Vultisig, Coldcard and
   SEAL all advise a test transaction first. One 2-of-3 guide has every signer *pair* sign
   once.
7. **Thresholds are understood through consequences.** Safe and Squads both advise
   1 < t < n and explain it as recovery ("a lost signer can be replaced"). The
   multi-device wallet study (ePrint 2022/075) found people's threat models drive the
   defaults they prefer, which argues for presets by situation.
8. **Multi-party setup needs visible status per person.** Bitkey's two-sided "Active",
   Apple Family Sharing's per-invitee status, and Life360's member preview before
   joining. I found no published research on the "waiting for co-signers" state itself,
   so treat our design there as practice, not evidence.
9. **People bring bank expectations.** Users expect payments to be "free, reversible,
   cancellable" (Voskobojnikov et al., CHI 2021). Say plainly, at the first payment, that
   a sent payment can't be undone. Also say that Zafe lets you cancel *before* it is sent.
10. **Warnings wear out.** Attention to repeated warnings drops sharply (Vance, Anderson
    et al.). Keep a small budget of strong warnings and make each look different.
11. **Don't gamify money.** Robinhood dropped its first-trade confetti under regulator
    criticism (2021). Zafe's quiet payment sounds are the right register.

## 4. Where Zafe stands (2026-10-14)

Already good, keep it:
- The live consequence sentence under the threshold steppers.
- The bearer-credential warning on invites, and invite links that never join by
  themselves.
- Setup resumes after the app is killed.
- The immediate backup prompt after keys are made.
- Recover's "compare the vault address with a co-signer" step.

Gaps, worst first:
1. **No way out after membership is locked.** Once sealed, setup shows only
   "Create vault keys", and the router forces a vault without keys back to `/setup`.
   One member who never taps strands everyone. Each attempt times out after 600 s, and a
   first-vault user can't even reach Settings.
2. **The safety number is weak, both as crypto and as a ceremony.** About 40 bits that a
   malicious relay can grind (audit M1). It's one local checkbox for the whole group,
   shown in hex fragments, with no QR mode. Spec §7.2 asks for more.
3. **Nobody is told the ceremony needs everyone, at once, for ten minutes.** Members
   learn it by getting stuck. Before keys exist there are no notifications, so a joiner
   who closes the app never hears that membership was locked.
4. **The backup reminder ranks below update and beta notices, and "Later" costs
   nothing.**
5. **The notification prompt arrives cold** on the first Home build. Denial makes a
   multisig member miss approval requests, and nothing tells them later.
6. **First deposit and first payment get no guidance.** The empty state also contradicts
   itself: "How about proposing your first payment?" with a zero balance.
7. **Members are hex fragments during setup**, so "check the member list" can't really be
   done.
8. **Four equal choices on Welcome**, two of them recovery paths whose difference means
   nothing to a newcomer.
9. **The spec's t = n confirmation is missing.** The app shows a warning line, not a
   confirmation.
10. Raw `$e` in four setup toasts. **Fixed 2026-10-15**: they use `zafeErrorMessage`.

## 5. The design

### 5.1 Principles

1. Onboard the vault, not the user. Every screen in setup shows where the *group* is.
2. Teach at the moment of use, in one sentence, with the reason. No up-front tour.
3. Gate on what protects funds (safety check, backup before funding), not on
   understanding.
4. Every waiting state names who we're waiting for, offers a nudge, and has an exit.
5. Protocol and UI change together where the UI is a security control.
6. A small warning budget: invite = bearer credential, safety check, backup,
   "can't be undone". Nothing else gets warning styling.
7. Everything we learn about a person stays on their phone (no telemetry).

### 5.2 Welcome

One sentence of promise, then two primary paths and one quiet one:
- **Start a vault**
- **I have an invite** (paste or scan)
- *I'm already in a vault* → "Do you have your backup file?" Yes → Restore. No → "Your
  co-signers can move your seat to this phone" (Recover).
- Text link: *See how it works (2 min, no money)* → practice vault (P2, §6).

Under the buttons, one line answers the biggest fear before anyone asks: "Lose your
phone? Your co-signers can move your seat. With a backup you restore it yourself."

### 5.3 Create

1. **Who's in it?** Presets by situation, with 2-of-3 selected:
   - "Two of us" (2-of-2, with the warning that both phones must sign every payment)
   - "Two of us, with a backup person" (2-of-3)
   - "Small team" (2-of-3), "Committee" (3-of-5)
   - Custom: the current steppers.

   Each preset shows the existing consequence sentence. t = n opens a confirmation sheet
   (spec §7.1) instead of orange text.
2. **Your name in this vault.** One field with a sensible default. It becomes the shared
   `Name` once keys exist (see §5.6), so the waiting room shows names, not hex.
3. **Vault name.** The approval window moves under "More options" (default 7 days).
   Today a newcomer must choose it before knowing what a proposal is.
4. **"Before you invite"**, a one-screen brief, not a tour:
   - Everyone needs Zafe on their own phone.
   - You'll all compare a short code on a call, then make the keys together. Plan
     10 minutes when everyone can join.
   - Toggle: *We're in the same room* (scan mode) or *We're remote* (call mode).
   - "Copy a message for your co-signers": install link + what to expect + the invite.

### 5.4 The safety check: protocol and ceremony together

**Protocol (fixes audit M1 and M2).** Today's number hashes the member keys, which the
relay can grind. Change it to a commit-then-reveal short authentication string, the ZRTP
and Bluetooth pairing pattern:
- Each member commits to a fresh random value when joining, then reveals it after the
  membership is locked.
- The short code is derived from the member keys plus all revealed values.
- No one can choose their value after seeing the others, so a relay grinding for a match
  must win a one-shot guess. Then 5 words (about 55 bits from a 2048-word list) is far
  more than enough, and shorter than today's 12 digits.

The seat-recovery code (audit M2, 8 digits, groundable in seconds) gets the same
treatment. Both are new message kinds, so they need a `DKG_ROUND1`/`INVITE`-style
version bump. Spec §5.3 and §7.2 change with them.

**Ceremony.**
- Show the code as **words**, never "pick the matching one" (Tan et al.).
- One sentence of *why* above it: "This catches a fake member slipped in by the server.
  It's the one check only you can do." (Vaziripour: 14% → 78% with the reason.)
- **Same room:** each phone shows a QR of its code; scanning another member's QR marks
  that member as compared. The device compares, not the eye.
- **Remote:** a call script. "Start a group call. One person reads the words; everyone
  else taps *Matches* or *Different*." Because the code is the same for every honest
  member, one reading on a group call covers the whole vault: O(n), not one call per pair.
- **Per-member state**, shared through the relay as signed envelopes: compared ✓ or
  pending. Keys can only be made once every member has confirmed (spec §7.2, which the
  app doesn't enforce today).
- *Different* is a real answer with its own screen: stop, nobody makes keys, the creator
  cancels the setup and sends a new invite.

### 5.5 Waiting room (between invite and keys)

One card per seat. Statuses: invited, joined, compared, ready for keys.
- The creator sees "2 of 3 joined. Waiting for Bob" with *Share invite again* and
  *Show QR*. A joiner sees who is in before accepting ("Alice's vault 'Grants', 2 of 3").
- Local notifications from the setup poll while the app is backgrounded ("Bob joined.
  Compare your code now"). Engineering: register the push token at join time so the
  relay can wake setup members too. Today pushes start only from Home.
- Invites expire (72 h, renewable). The invite is a bearer credential until sealed;
  expiry and *Revoke* bound that.
- **Exit after sealing.** Any member can *Abandon setup*. If keygen times out, a screen
  says which members never took part, and offers *Try again* or *Abandon* (fixes gap 1).
- Make keys automatically once everyone has confirmed. There's no separate
  "Create vault keys" tap, so there's no member left to wait for.

### 5.6 Right after keys exist

1. **Backup, as the main path.** "Back up now" is the primary button. "Later" asks for an
   acknowledgement written for the group: "If I lose this phone without a backup, the
   vault keeps working only while no more than n−t members lose theirs."
2. **Recall check.** Ask for the passphrase back once, right after export. Export already
   decrypts the file; this adds the human part. Then Signal-style spaced checks as a quiet
   Home card (1 d, 3 d, 7 d, then monthly; "I use a password manager" turns them off).
3. **Notifications primer**, then the OS prompt: "Co-signers will need your approval.
   Zafe tells you when a payment waits for you; notifications show no amounts on the lock
   screen." If denied, a persistent row on Signers and Home.
4. **Unlock primer**, one line: "Zafe asks for your fingerprint before approving."
5. Publish the member's chosen name (`VaultEvent::Name`).

### 5.7 Vault readiness, then the first deposit

Home's empty state becomes a readiness card: status first, one action (NN/g empty
states; Casa's health check).

| Check | Source |
|---|---|
| Everyone compared the code | Setup (always true once keys exist) |
| Members with a backup: k of n | `BackupVerified` in the log |
| Every member's phone reachable (pool published) | Log pools |
| Test payment done | Proposal history |

- **Receive** stays available, but while this member has no backup it shows an
  interstitial first ("Back up before the vault holds money"). That's a soft gate; I
  rejected Bitkey's hard gate because a DAO treasurer funding the vault shouldn't be
  blocked by one member.
- **First deposit:** "Send a small amount first." Name which wallets can pay an Ironwood
  address. **Unverified:** check which wallets support Ironwood before writing this copy.

### 5.8 First payment and first approval

- **Guided test payment** from the readiness card: a small payment to the vault's own
  address. Members' checks accept payments to the vault, as sweeps show. It walks
  through propose → approve → auto-send with real signatures. For 2-of-3, offer "Try
  another pair".
- **One-time tips at the moment of use** (TipKit pattern), replayable from Settings > Help:
  - first approval request: "Your tap signs. It can't be taken back."
  - first auto-send: "Your phone sent it because your approval was the last one needed."
  - first review: "Once sent, a payment can't be undone. Until then, the proposer can
    cancel it."
  - first received payment: "Every member can see this."
  - first time a member is unreachable: what one-tap needs.
- Fix the empty-state copy: no "propose your first payment" while the balance is zero.

### 5.9 Architecture

- **Per-vault onboarding record**, local only: `<vaultDir>/onboarding.json` (compared,
  recall-checked, notifications asked, tips seen, test payment id). Derive from the log
  wherever possible (backups, names, pools, proposals). The JSON holds only what the log
  can't know. It's an app cache like `summary.json`, rebuilt by the app.
- **Setup journal:** extend the existing resume-to-`/setup` with explicit states
  (joined → revealed → compared → keygen → backup) so every state has a screen and an
  exit.
- **Tips registry:** ids, a condition over app state, a seen flag in prefs. One widget
  renders them. Settings > Help lists them.
- **Practice vault** (P2): a fake `ProposalsState`/`VaultState` driving the real screens,
  never touching Rust or the network. The render-test fixtures
  (`tool/screens/*_render_test.dart`) already pump these screens with plain data.
- **No new network calls for onboarding**, except setup-time push registration and the
  commit/reveal and confirmation envelopes in §5.4.

## 6. Priorities

| P | Item | Prevents |
|---|---|---|
| P0 | Exit after sealing + keygen timeout screen (§5.5) | Stuck group |
| P0 | Backup card ranked first once funded; acknowledgement on "Later" (§5.6) | Lost funds |
| P0 | Notification primer + denied state (§5.6) | Missed approvals |
| P0 | Empty-state copy fix (§5.8) | Confusion |
| P1 | Commit-then-reveal code, words, per-member confirmation, QR scan (§5.4) | Impostor vault (audit M1/M2) |
| P1 | Waiting room with per-seat status, setup notifications, invite expiry (§5.5) | Stuck group |
| P1 | Presets, t = n confirmation, names in setup, brief (§5.3) | Bad threshold, hex members |
| P1 | Readiness card, Receive interstitial, recall check (§5.6, §5.7) | Lost funds |
| P2 | Guided test payment, tips + Help hub (§5.8) | First-payment mistakes |
| P2 | Practice vault (§5.9) | Evaluators can't try it (hypothesis: test first) |
| P2 | Welcome simplification (§5.2) | Confusion |

## 7. Critique of this design, and what I'd still change

- **Auto-starting keygen** removes a tap but also removes a moment of consent. Keep a
  final "Make keys" step only for the *creator*, who already holds the group's attention.
- **Spaced backup checks** could annoy treasurers who keep passphrases in a password
  manager. The opt-out is one tap; Signal's data says about a quarter will take it.
- **The group-call reading** relies on everyone actually listening. Per-member
  *Matches* taps make each person commit, which is the best we can do without
  co-presence.
- **Setup-time push** puts the device's push token on the relay before the vault exists.
  It's content-free and the relay already sees the member key, so no new link is created.
  It still widens what the relay knows during setup; say so in spec §6.
- **The practice vault** is the most visible item and the least proven. No controlled
  study shows sandboxes improve first-time success; vendor data (Alpaca paper trading) is
  weak. Build it after the P0/P1 items, and only if sessions show evaluators need it.
- Not designed here: iOS specifics, desktop, the website's `/join` page as the joiner's
  first screen (it should tell people without the app to install it, then tap the link
  again, since there's no deferred deep linking without a third-party SDK).

## 8. How we'll know it works (without telemetry)

- **Moderated sessions:** recruit groups of 2-3 who haven't used Zafe and have them make
  a testnet vault remotely, on a call, while we watch. Measure time to keys, where people
  stall, whether they read the code aloud, and whether every member backed up. Five
  groups find most problems.
- **Concierge setups** for the first DAO and grant customers (Unchained/Superhuman
  style). Each one is also a research session.
- **The opt-in diagnostic report** already records slow steps and errors on the phone;
  users share it by hand.
- **Re-run the safety-check study in miniature:** in one session per round, inject a
  mismatched code and see who catches it.

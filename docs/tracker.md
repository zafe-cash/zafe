# Zafe tracker

What's left, what's deferred, and ideas worth keeping. Read it at the start of a task and
update it in the same change when you finish, defer or find something. `§` refers to
`spec.md`.

- Open items say exactly what is left. Done items are one line: what, when, where the
  details live. Never delete a done item: shorten it.
- Each fact lives in one item; link to it instead of repeating it.
- Legend: `[ ]` open · `[x]` done · **(you)** needs the user · *(idea)* not decided ·
  *(check)* not verified, look before relying on it.

Last updated: 2026-10-27.

## Now: relay moves (merged 2026-10-27)

`relay-client` was merged into `main` on 2026-10-27 (`9724b0c`): the testnet relay
(`RELAY_DB` 4, `/v1/info`) and the site are deployed. Details: `docs/relay-move.md` §7.

Ship order (relays decode envelope kinds, so an old relay rejects move messages):
- [x] Merge `relay-client` into `main` (2026-10-27). Testnet relay and site deployed.
- [ ] **The live docs are ahead of the app**: zafe.cash/docs shows the Server screen,
      server moves and the new Settings, which 0.1.22 doesn't have. Release the app soon.
- [ ] Promote the relay to mainnet (`relay-mainnet.yml`) before any build from this branch
      reaches users.
- [ ] Promote after 24 h on testnet: the testnet relay went live 2026-10-26 21:00 UTC.
- [x] `v0.1.24` testnet dry run passed on a phone (Android 16), 2026-10-27: in-place update,
      create 2-of-2 with a CLI member, safety words, backup, wipe + restore, receive 1 TAZ,
      propose 0.1 TAZ with a memo, CLI verifies and approves, Send now, mined (txid
      `f966e885…`). 0.1.23 had failed it: one key press veiled the app in grey (fixed).
- [ ] `v0.1.25` (0.1.24 + menus, restore Paste, clipboard countdown): check the changed
      screens and one send on a phone, then `mainnet-v0.1.25` on the same commit, after the
      mainnet relay is promoted (0.1.24 stays testnet only).
- [x] Restore: **Paste** says when the clipboard is empty or holds something that isn't a
      backup (and never pastes that into the field); it used to do nothing (2026-10-27).
- [x] Backup **Copy as text** and **Copy words** show "On the clipboard for 42 s", then "No
      longer on the clipboard" (`ClipboardCountdown`, 2026-10-27).
- [x] Menus (`AppMenu`) place themselves: hang from the anchor's trailing edge, inside 16 px
      margins on any screen and text size (MenuAnchor ran past the margin, or off a small
      phone at 2x). Manage vaults' ⋯ no longer repeats "How it's protected" (2026-10-27).
- [ ] *(upstream)* The Valar testnet faucet pays nobody since 2026-10-23 ("the wallet
      cannot pay this address" for t- and shielded addresses):
      https://github.com/valargroup/valar-testnet-faucet/issues/5. Testers need another
      source of TAZ meanwhile; releasing.md points at this faucet.

Done on the branch:
- [x] Relay: labelled mailboxes, `GET /v1/info`, `/v1/push/forget`, mailbox expiry,
      create token, `RELAY_DB` 4 (2026-10-25).
- [x] Client foundations (phase 2a): per-vault relay in `VaultMaterial` 2, random labels,
      `INVITE` 3, `BACKUP` 3 with the log copy, `REPAIR` 3 anchors, seat links (2026-10-25).
- [x] App (phase 2b): server picker, per-vault server, create token, outage status, seat
      links (2026-10-25; `docs/app-notes.md`).
- [x] Moving a vault (phase 3): `VaultMoved` (`VAULT_EVENT` 10, gated), sealed arrivals,
      notices and cancels, CLI `move`/`follow`, app screens; tests `tests/relay_move.rs` (16),
      `scripts/move-e2e.sh` (2026-10-25).
- [x] Emulator end to end, the app as member A with harness members B and C on six relays:
      setup, follow, move, payments across switches, restore onto the moved relay, seat
      recovery by link, create on a picked relay. Four bugs fixed (2026-10-26; `e1def69`,
      `53e7e54`).
- [x] UX rework: "server" in the UI; one Server screen per vault with Move and a waiting
      room (Nudge per member); moves arrive as Needs you cards; a Status screen that
      suggests a listed server after 6 h down; Settings first level (Privacy, Security, the
      vault, This phone, App); full screens instead of stacked sheets; `AppMenu`,
      `showAppConfirm`; Home pull to refresh (2026-10-27; `ee60bab`).
- [x] Plain balance and payment cards; sync status green when synced, orange on failure,
      purple for Tor (2026-10-27; `c4130d5`).
- [x] Theme pair lint `scripts/check_theme_pairs.py`, CI job `theme-pairs` (2026-10-27).
- [x] Six-size type scale with tabular amounts; `scripts/check_typography.py`, CI job
      `typography`; rules in `docs/brand.md` "Type" (2026-10-27).
- [x] Site docs: 17 pages under `/docs` (`infra/site/src/docs/pages.ts`), including
      `/docs/run-a-server`; `/relay` redirects there for good; one nav (Docs, Security |
      mark | Support, Get Zafe); `scripts/check_docs_sync.py` and
      `scripts/check_docs_links.py` in CI (2026-10-27; `docs/site.md`).

Open from moves:
- [ ] Leaving a fork after a move: a write that races the switch forks the old relay, and
      `follow_move` then fails with `RelayForked`. No screen offers `node::follow_relay`
      for the new relay.
- [ ] The mover's waiting room updates only on the next vault refresh (up to a minute in
      the emulator run): refresh it when an arrival lands.
- [ ] Site page `zafe.cash/move` (opens the app with the fragment; explains a move to
      someone without Zafe), plus verified App Links and Universal Links for `/move`.
      Today the link opens the app through an unverified intent filter.
- [ ] A second listed server per network (a standby relay at another provider, with a
      restore drill; `docs/relay-move.md` §5). With one preset per network in
      `relay_presets.dart`, the Status screen's 6-hour move suggestion
      (`suggestedServer`) never finds another server, so it never shows.
- [ ] A recovered seat without a pool (seen 2026-10-26, CLI B2 after `recover --wait`):
      C approved one-tap, B2 interactively (no pool yet), and `send`/`request` refused
      ("membership is not ready") until the proposal was cancelled. Check whether a
      recovered app can hit it (the app tops up its pool on its first refresh) and whether
      `request` should take a one-tap voter's part.
- Known limit: after a switch a phone stops reading the old relay's notices, so only
  stragglers and followers detect `TwoMoves` (`docs/relay-move.md` §9).

## Mainnet and releases

State on 2026-10-27: mainnet beta on Google Play (live since 2026-10-16) and as GitHub
APKs (newest `mainnet-v0.1.22`, 2026-10-22); `relay.zafe.cash` last promoted 2026-10-22;
no external audit yet.

- [ ] **(you)** External security audit (`docs/audit-scope.md`); record the audited
      commit here. It is the last mainnet gate; the beta label stays until then.
- [ ] NU7 activates on mainnet at block 3,543,405 (about 2026-11-05). Watch the first
      vault sends after it. Releases from 0.1.18 support it (`/docs/updates`).
- [ ] *(check)* Record the Play update test 0.1.16 → 0.1.21 planned on 2026-10-22.
- [ ] *(check)* See a real "needs your approval" push on mainnet.
- [ ] Restore the mainnet relay DB from R2 on another machine and open it. The
      2026-10-12 restore ran on the VPS only, and the image has no `sqlite3`.
- [ ] A separate host for mainnet (testnet and mainnet share one VPS).
- [ ] **(you)** Delete the local relay key copies in `~/.config/zafe/relay-keys`
      (`shred -u`).
- [ ] **(you)** *(check)* Back up the upload key and its password in a password manager.
- [ ] Testnet invite page: "Open in Zafe" still opens `zafe://`, which both apps claim.
      Testnet builds have read `zafe-testnet://` since at least `v0.1.17` (2026-10-16), so
      this can ship: in `JoinPage.astro` add `data-scheme={testnet ? 'zafe-testnet' :
      'zafe'}` to `<a id="open">`, and in `public/assets/join.js` build the link from that
      attribute instead of `'zafe://join?invite='`.
- [ ] Built-in Kotlin, once `firebase_core` and `workmanager_android` stop applying KGP.
- [ ] **(you)** Post the preview (Zypherpunk Discord) and a second post.

Done:
- [x] NU7 port (found 2026-10-12, finished 2026-10-21): librustzcash NU7 pre-release
      line, `MAINNET_NU7_HEIGHT` set by us, a propose-time branch check, expiry cut before
      the upgrade; testnet send verified on a phone 2026-10-13; `mainnet-v0.1.21`
      (essential) promoted to Play production 2026-10-21; heads-up in the site FAQ and
      release notes. AGENTS.md "Upstream status".
- [x] Mainnet relay (2026-10-12): `relay.zafe.cash`, Litestream to an EU R2 bucket,
      `relay-mainnet` environment with a reviewer, separate deploy users and keys; FCM key
      added 2026-10-12; installation-ID pushes (`FcmFid`) promoted 2026-10-22 (the last
      testnet deploy before it was `57876ab`). `infra/relay/README.md` "Mainnet".
- [x] Mainnet dry runs with real funds (2026-10-12): CLI co-signer, then two phones;
      backup, remove, restore; the `PoolReset` fix shipped as `mainnet-v0.1.2`. Not
      covered: a 3-of-5 vault, sending through Tor on mainnet, iOS.
- [x] Seat move and share repair on a phone (testnet, 2026-10-16/17), the phone as helper
      and as the new device. A repaired share signed in the emulator run (2026-10-26),
      not yet on a phone.
- [x] Release pipeline: `release.yml` (testnet `v*`), `release-mainnet.yml` (`mainnet-v*`:
      APK and Play internal AAB), `play-promote.yml` (production; the update priority
      comes from `docs/releases/<version>.md`). `docs/releasing.md`.
- [x] Google Play listing live (2026-10-16); `zafe.cash/.well-known/assetlinks.json`
      lists two certificates per package (checked 2026-10-27).
- [x] Testnet app `xyz.zafe.zafe.testnet` with invite links on `testnet.zafe.cash`
      (serves `assetlinks.json`, checked 2026-10-27).
- [x] Flutter 3.47.7, AGP 9.1.1, FCM installation IDs, edge-to-edge, large screens and
      foldables (merged 2026-10-22, shipped in 0.1.22). Seen on Android 14/15 and Pixel
      Fold emulators; not on a real Android 14 phone or Fold/Flip hardware.
- [x] First-sync stall on phones (2026-10-12): the wallet key is derived from the identity
      seeds, not read from secure storage; the sync animation runs at about 16 updates/s
      (0.1.8-0.1.10).
- [x] Cancel a vault setup (2026-10-12): the creator deletes the mailbox; joiners leave.
- [x] Mainnet beta: a "beta" label, no vault or fund cap (decided 2026-10-07).

## App

- [ ] Android Back with the keyboard open pops a pushed screen (seen on Send and the server
      list over adb). Check with a real gesture before changing anything.
- [ ] Proposals refresh takes 6-11 s over Tor (pool top-up and log read are sequential
      round trips): batch or parallelise the relay requests.
- [ ] The vault's approval window isn't shown anywhere after creation.
- [ ] Zcash server failover: switch back to the chosen server when it recovers; fail over
      in background checks too (`docs/app-notes.md` "Zcash server list").
- [ ] Background checks don't watch the mempool (only the foreground does).
- [ ] A member whose phone lost its pool nonces approves interactively; show that their
      approval still needs a signing round.
- [ ] Rename a vault: carry the local name in backups. A vault-wide name needs a signed
      event and a `VAULT_EVENT` bump *(idea)*.
- [ ] Batch payments: edit a recipient in place (today: remove and re-add).
- [ ] "Check my backup" later (type the passphrase again).
- [ ] Import a vault as view-only (auditor mode from a UFVK).
- [ ] Address book via `ADDRESS_BOOK` proposals (t approvals, no FROST); warn on unknown
      recipients (§11.2).
- [ ] Rules via `RULES` proposals: per-transaction and per-period limits, allowlist-only,
      extra approvals for large payments; apps enforce them before voting and signing
      (§11.1).
- [ ] Transparent and TEX recipients with a warning (the spec allows them; `verify` is
      Ironwood-only).
- [ ] Onboarding P2: guided test payment, one-time tips and a help hub, a practice vault
      (test with users first). The notification primer isn't on the Signers tab yet
      (`docs/onboarding.md`).
- [ ] The unlock gate is UI only: key material isn't bound to user authentication.
      Binding it (Keystore `setUserAuthenticationRequired`, Keychain `.userPresence`)
      conflicts with background round-2 signing (V8).
- [ ] Wallet DB key: derived from the identity seeds, which sit in the same secure storage
      as the vault material, so SQLCipher protects against file copies, not against
      someone who can read the Keystore.
- [ ] Low-end device benchmark (V7: a Cortex-A55-class phone).
- [ ] Play in-app update from the production track, end to end (checked on the internal
      track only).
- [ ] The Nothing launcher files Zafe under "Other" (`appCategory` has no finance value);
      it may follow the Play category *(check)*.
- [ ] Agent-device flows are manual: script them (`scripts/app-harness.sh` covers the
      backend). Model: Vizor's `integration_test/payment_uri_prefill_test.dart`.

Never checked on a real phone:
- [ ] Invites: cold-start deep link, the camera permission-denied state, live camera QR
      scan (invite and recipient), an https invite link with Zafe not installed.
- [ ] Payment sounds and haptics (the emulator runs `-no-audio`); proving time from logs.
- [ ] Tor in background checks (WorkManager and FCM time budgets), dormant/resume,
      battery.
- [ ] Viewing key, history CSV export, CSV recipient import, app lock.
- [ ] Diagnostic report: a Rust panic and a background-engine error.
- [ ] A TalkBack pass.
- [ ] Launcher icon (themed), splash and notification tint.

## iOS (never built)

- [ ] Build and run at all.
- [ ] Associated Domains (`applinks:<host>`) and the team's app ID in the AASA
      (`ZAFE_IOS_APP_IDS`).
- [ ] Exclude the nonce directory and `<appSupport>/tor` from backups
      (`isExcludedFromBackup`).
- [ ] Platform channels: `xyz.zafe/payment_feedback` (AVAudioPlayer; the ringer switch
      silences it) and `xyz.zafe/modal_corners` (port Vizor's `NativeModalCorners`).
- [ ] Clipboard sensitivity flag, screen-capture shield, screenshot warning, app-switcher
      privacy overlay.
- [ ] APNs sender on the relay and a notification service extension.
- [ ] An update prompt from the App Store version.
- [ ] `permission_handler` needs `PERMISSION_CAMERA=1` in the Podfile's
      `GCC_PREPROCESSOR_DEFINITIONS` if we ever request through it (today only
      `openAppSettings`; `mobile_scanner` asks itself).
- [ ] When iOS ships: email the iPhone waitlist, then delete the `zafe-waitlist` store (the
      privacy policy promises it).
- *(idea)* CommonCrypto instead of vendored OpenSSL for SQLCipher (smaller binary).
- *(idea)* Keychain `first_unlock_this_device` for shares from day one (Vizor's
      `KeychainAccessibilityMigrator.swift`).

## Relay

- [ ] `catch_relay_up` across seat moves (open since the 2026-10-07 audit, §9).
- [ ] Other members can't resend a dropped transaction (only the broadcaster has the
      signatures and proof); anyone could rebuild a one-tap proposal from the log *(idea)*.
- *(idea)* Compact the log (old commitment batches); hide the author key in entry headers.
- *(idea)* The leader polls its inbox twice a second while collecting shares; it could use
  `/v1/wait`.
- *(idea)* Paid hosted tiers (D3 limits).
- *(idea)* Relay as a Tor onion service; per-vault Tor circuits; pooled relay connections
  over Tor (needs the arti client, which upstream's `Client` doesn't expose).

## Site

- [ ] `/showcase` reports no LCP (its title card fades in); re-measure the two ~42 ms
      hitches that sat at the old island switches.
- *(idea)* Double opt-in for the waitlist.
- *(idea)* Show a second real payment (with a memo); a changelog or "last updated".
- *(idea)* An illustrator's painted textures and paint-over for the loops.

## Upstream and waiting on others

- [ ] **(you)** Post the Discord pointer in `#frost` (`upstream-asks.md`); follow up.
- [ ] **(you)** Answer Daira's question on how ZIP 2005 could be clearer (draft in chat,
      2026-10-01); offer a zips PR.
- [ ] ZIP 2005 vectors cross-check: frost-tools#609 and PR #610 (2026-10-04), open,
      awaiting review (checked 2026-10-27).
- [ ] FROST book `book/src/zcash/technical-details.md` says to throw `sk` away and that the
      key share is the only secret (against ZIP 2005): send a docs PR.
- [ ] U4: COCKTAIL-DKG with Pallas (frost#1033). Align our `sk` agreement with the ZIP 312
      draft (zips#895, "Contributory Generation of sk") when it is usable.
- [ ] RedPallas FROST moves to the FROST repo (frost#963): switch crates when it ships.
- [ ] frost#1082 (resharing): answered the revocation question 2026-10-04
      (https://github.com/ZcashFoundation/frost/issues/1082#issuecomment-5982780783);
      follow up if ZF wants a spec.
- [ ] frost-tools#433 (send the group description and threshold to DKG participants):
      design proposed 2026-10-03; waiting for answers to its two questions before a PR.
- [ ] ZIP 321 link security (zodl-project#60): proposal posted 2026-10-04
      (https://github.com/zodl-inc/zodl-project/issues/60#issuecomment-5982699354);
      nothing built until they answer.
- [ ] `ths start --detach` (thus-spoke-zakura#147, PR #308, open on 2026-10-27): once
      released, `up.sh` runs `ths start --detach` and `down.sh` `ths stop`.
- [ ] ths 0.4.1 is out (2026-10-21); `infra/regtest/up.sh` pins 0.4.0.
- [ ] Zakura's faster prover (V7): supports Ironwood since zakura-core/common 2.2.0
      (2026-09-30); desktop bench 2.85 s → 0.57 s for a warm 2-action proof. Adopting it
      means the whole `zakura-*` stack (as Vizor did). Next: a spike branch,
      `scripts/android-bench.sh`, regtest e2e and the zcash-sign cross-check.
- [ ] Stack plan: become the shared multisig layer other wallets build on. Tracked in
      `docs/stack-plan.md` (research: `docs/stack-design.md`), not here.

## Later (M3/M4, §16)

- [ ] Rotation with the same address and t (§10.2): add members by repair toward a new
      identifier (§10.4.2; on our pins 2-of-3 → 2-of-4 signs once members add the new
      verifying share, checked 2026-10-04); remove members by DKG refresh (§10.4.1).
- [ ] Change t on the same key by resharing (§10.4.4): our own code (not in frost-core,
      frost#1082), spec and external audit first; reference test
      `crates/zafe-core/tests/reshare.rs`. Old shares kept by a removed member still sign
      at the old t: rotation is for friendly changes only.
- [ ] One-time ceremony encryption keys (§10.4.4): forward secrecy for DKG, refresh,
      reshare and repair messages; must ship before refresh or reshare.
- [ ] Migration to a new vault (hostile removal; change t until resharing ships) (§10.3).
- [ ] Desktop builds (Vizor has desktop layouts).
- [ ] dApp SDK: proposal requests from other apps.
- [ ] Hardware members (Keystone, Ledger) if they gain FROST support.
- [ ] Payment disclosures (ZIP 311-style proofs for auditors).

## Tech debt and gotchas

- [ ] Messaging crypto upgrade (`ed25519-dalek` 2, `hpke` 0.12, `chacha20poly1305` 0.10): the
      `bip32` clash on `sha2` 0.11 is gone (bip32 0.6). Check each crate's wire output with
      the existing vectors before bumping.
- [ ] The bridge serializes wallet DB access with one global lock; revisit for parallel
      syncs of several vaults.
- [ ] A proposal's `created_at` is the proposer's clock (display only, untrusted).
- [ ] *(check)* The live activity watch (`/v1/wait`) and the background-check ANR fix were
      never looked at on a phone.
- Debug-signed builds (`flutter run`, profile) never get verified app links, and
  `flutter run` uninstalls and reinstalls on a signature mismatch, wiping the app's data.
  Use `adb install -r` with the release key when data matters.
- On the emulator the app sometimes shows a ~12% white veil (#262929 instead of #080B0B)
  over the Flutter surface, outside the app's view tree; cause unknown.

## Decisions

- Brand: Verdigris palette and the Seam mark (2026-10-01, `docs/brand.md`).
- Backups are passive copies; moving to a new phone = restore there, then Remove on the old
  one (2026-09-30).
- Member tiles are derived from keys; names are labels shown beside them (2026-10-01).
- One-tap and interactive approvals never mix; a payment no group can complete offers
  "Propose again" (2026-10-15).
- Mainnet beta: label only, no caps (2026-10-07).
- Setup: no threshold presets, no t = n sheet, no "before you invite" brief, no same-room QR,
  no readiness card or Receive interstitial (2026-10-16, user).
- Relays: "one relay per vault, no migration" (2026-10-06) was reopened on 2026-10-25:
  per-vault relays and moves (`docs/relay-move.md`). Push from other relays (UnifiedPush)
  is deferred (§10 there).
- Private transaction-detail lookups via PIR: deferred (checked 2026-10-14). They need the
  Zakura wallet-library forks and a third-party service; nothing leaks today. Revisit when
  those publish releases or the vault needs memos on received notes
  (`docs/vizor-reference.md` 12.6).
- Every old decoder stays once external testers exist (AGENTS.md "Bumping a version").

## Ideas (not decided)

Paths in parentheses are in Vizor's repo (chainapsis/vizor-wallet) unless they start with
`docs/` here.

- N-of-M ring emblem (M segments, T filled) and a Quorum Z splash animation
  (`docs/brand.md` §6).
- Dollar values beyond the balance card (payment amounts, send review).
- "Keep screen awake" while collecting signatures (`sync_keep_awake_provider.dart`).
- Show which signers are online or last seen (relay metadata; a privacy trade-off).
- Explorer links for txids with a custom explorer setting (`zcash_explorer.dart`).
- Comments on a proposal, in the encrypted log.
- Scheduled and recurring payments (Vizor's background outbox of signed transactions).
- "Request ZEC": a ZIP 321 request builder that never emits `label`/`message`.
- "Pay with Zafe" verified link `https://zafe.cash/pay#<zcash: URI>`: only the real Zafe
  opens it. Build when the first dapp or merchant asks.
- Propose from a request: a `zcash:` URI card that prechecks and builds the proposal.
- One intake for `zafe://` invites and `zcash:` URIs (`incoming_link_dispatch.dart`).
- A compact positional invite payload (`docs/compact-gift-links.md` in Vizor).
- Fee breakdown for batches: "per recipient × N + network fee = total".
- Unknown broadcast results: "Check status" (scan only), never a second send.
- Durable checkpoint of the signed transaction before broadcast, with recovery at startup.
- Honest signing stages from protocol boundaries, no timers or percentages.
- Evidence-based payee states ("Confirming", "Unverified" with a reason).
- Contact names on every address surface (basis for the address book).
- Avatar step after creating or joining a vault.
- Android back-exit guard; iOS number-pad Done bar.
- Widgetbook galleries and headless PNG renders for design review.
- Multi-part QR to move a vault between phones without a file.
- Reproducible / F-Droid builds (Vizor's `scripts/build-android-reproducible.sh`).

## Done before 2026-10-25

One line each; the commit history and the linked docs have the detail.

App, M1 (spec §16):
- [x] Flutter + FRB app on Vizor's architecture: create, join, keygen, receive, balance,
      proposals, on-device check, approve/reject, async signing, send (`c07c687`,
      `c508aff`).
- [x] Proposal expiry: per-vault approval window (1/3/7/14 days at creation, 1-30
      accepted), expiry heights rounded to 144 blocks, "Expired" and "Propose again"
      (2026-09-30).
- [x] One-tap approvals and auto-send (§9.5.2); pools published at the end of keygen;
      groups need only their own members' pools (`VAULT_EVENT` 6, 2026-10-07).
- [x] Notifications: relay FCM pushes on every log append, background checks
      (WorkManager, FCM wake-up), dead tokens pruned (2026-09-29 to 2026-10-12).
- [x] Tor: "Use Tor" per phone, fail-closed for lightwalletd and the relay (2026-10-01;
      `docs/app-notes.md` "Tor"). Passed on a phone 2026-10-16.
- [x] Incoming payments and pending receipts from the mempool (2026-09-30; on a phone
      2026-10-17).
- [x] Invites: QR scan, `zafe://` and https links (App Links on `zafe.cash`), invite in the
      URL fragment (2026-09-30 to 2026-10-01); 72 h invite expiry (2026-10-16).
- [x] Testnet dry run on a phone (2026-09-30): txid `e6ccd6b7…0237` at block 4,422,289.
- [x] TLS everywhere (rustls, bundled roots) and relay packaging (2026-09-30).
- [x] Relay hosted on an OVH VPS, deployed by `relay-deploy.yml` (2026-10-06); deploy
      keys split per network (2026-10-11); GHCR image public (checked 2026-10-27).
- [x] Relay rate limits, storage quotas, creation caps, inbox ack, long poll `/v1/wait`
      (2026-09-30 to 2026-10-01).
- [x] Cancel, "Make it unsendable" sweeps, "Start over" with other signers, note
      reservation (log rule and wallet holds), rebroadcast of dropped transactions
      (2026-09-30).
- [x] Member names: local labels and shared `Name` events (2026-09-30 to 2026-10-01).
- [x] Unlock gate, wallet DB on SQLCipher, proving-key prewarm, payment sounds and
      haptics, Play in-app updates, tabs, vault emblems, app icon and splash (2026-09-30 to
      2026-10-06).
- [x] Multiple vaults per phone, one identity per vault, encrypted export/import,
      viewing-key export, backup health in the log (2026-09-30 to 2026-10-01).
- [x] Batch payments (up to 50, CSV import), CSV history export, recipient QR and ZIP 321
      payment links, `zcash:` links from other apps, app lock (2026-09-30 to 2026-10-04).
- [x] Seat repair without a backup (§10.1, §10.4.2), safety words for recovery
      (2026-10-01, 2026-10-15).
- [x] Zcash server list with latency and failover (2026-10-06).
- [x] Unapproved-spend alert (`spend_watch`, 2026-10-07).
- [x] Relay rollback and loss checks, restore onto a relay, "Follow the relay" (2026-10-07;
      AGENTS.md "Rollback and loss").
- [x] Versioned formats with `UnsupportedVersion`, "Update Zafe" notices (2026-09-30).
- [x] No telemetry: CI guard and a local scrubbed diagnostic report (2026-10-07).
- [x] Vizor core resynced to `4bff2e7` (`docs/vizor-reference.md` §11).

Security audit and races (2026-10-14, fixed by 2026-10-15; `docs/audit-scope.md`):
- [x] One-tap vote kept before append, duplicate-send detection, envelope seq re-seal,
      former members' entries accepted (H1), M1-M5 and L1-L10 fixed (commit-reveal safety
      words, `VAULT_EVENT` 9, `SilentSigners`, encrypted nonce files, ...), app hardening.
- [x] `sk` agreement committed in DKG round 1 (`DKG_ROUND1` 3, 2026-10-01).

Onboarding (`docs/onboarding.md`):
- [x] P0 and P1 done by 2026-10-16: typed error copy, keygen timeout naming who's missing,
      async setup (nobody online at once), backup first once funded, notification primer,
      per-member safety words, invite expiry, name in setup, recall check. P2 Welcome
      simplification done 2026-10-21.

Site (`docs/site.md`):
- [x] Astro site with a strict CSP, `/join`, SEO (`docs/seo.md`), Vercel deploys from
      `site.yml`, domain `zafe.cash` (registered 2026-10-01, expires 2027-10-01; move to
      Cloudflare Registrar after the 60-day lock) (2026-10-01).
- [x] Home rebuilt around one payment, `/download`, `/support`, iPhone waitlist
      (2026-10-22); illustrated loops (2026-10-23); `/showcase` as one smooth story
      (2026-10-23).

Upstream:
- [x] ZF questions U1, U2, U3, U5 answered (frost#1094, Discord; 2026-09-21 to
      2026-10-01; `upstream-asks.md`).
- [x] thus-spoke-zakura #124 (Ironwood fix) merged 2026-10-01; #145/#146 (`ths stop`)
      fixed in 0.4.0; regtest runs through `ths` since 2026-10-04.

CI:
- [x] `ci.yml`: fmt, clippy `-D warnings`, tests, ZIP 2005 vectors, no-telemetry, Flutter
      analyze and test (docs sync, typography and theme pairs come with `relay-client`).
      Regtest and Docker tests stay manual.

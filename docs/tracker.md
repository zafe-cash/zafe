# Zafe tracker

What's left, what we deferred, and ideas worth keeping. Update it whenever something is
decided, deferred, discovered or finished. Agents: add items you leave behind; don't delete
finished ones, tick them and add the commit. Spec references are to `spec.md`.

Legend: `[ ]` open · `[x]` done · **(you)** needs the user · *(idea)* not yet decided

Last updated: 2026-10-06 (relay CI/CD deploy to the OVH VPS; relay loss/rollback gap; stack plan; Verdigris + Seam brand implemented; note reservation; versioned formats; relay TLS + packaging; wallet DB encryption; incoming payments; pending receipts from the mempool; https invite links + landing site; Astro landing page)

---

## Next up (proposed order)

- [ ] **Stack plan** (2026-10-01): become the shared multisig layer other wallets build
      on (crate split, upstream PRs to ZF/librustzcash, open spec + draft ZIP, Zodl POC).
      Checklist: `docs/stack-plan.md`; research: `docs/stack-design.md`. Track its items
      there, not here.

- [~] **Open source + testnet preview** (2026-10-01): history scanned with gitleaks and
      trufflehog (0 verified secrets; the one hit is the relay's test-only RSA fixture);
      the unrelated `colosseum-copilot` skill left the repo (now in `~/.claude/skills`);
      the site's calls to action say "View code" and link the repo until
      `ZAFE_DOWNLOAD_URL` is set.
  - [x] The repo is public (2026-10-01, rescanned first); description and homepage link
        the site. Moved to `zafe-cash/zafe` on 2026-10-03.
  - [ ] **(you)** Post the preview (Zypherpunk Discord).
  - [x] Testnet APK in GitHub Releases: `v0.1.0` pre-release is published (workflow run
        green, 16 min). `site.yml` now sets `ZAFE_DOWNLOAD_URL` to the Releases page
        (`/releases`: `/latest` skips pre-releases), so "Get Zafe" returns on the next
        site deploy (2026-10-07).
  - [ ] **(you)** second post.

- [x] **Brand: Verdigris + Seam** (decided 2026-10-01, `docs/brand.md`; implemented on
      branch `worktree-agent-a762a0ef73fae7642`): palette tokens (generated,
      `scripts/brand/primitives.js`), light vault card in light mode (`colors.vaultCard`),
      gold for received/success, Patina icons for Home, Activity, the tab bar, activity
      rows, the notice card and Home buttons, Seam app icon / splash / notification icon,
      window colours, illustration palette, retinted vault emblem palettes.
  - [x] **Patina for the rest of the icon set** (2026-10-01): all 46 used icons are
        Patina (Phosphor via `scripts/brand/patina_icons.py`), except the Zcash currency
        glyph; 65 unused SVGs (Vizor leftovers incl. `vizor_logo`, keystone, ledger,
        dragon) and 5 unused PNGs were deleted. Contact sheet:
        `tool/screens/icons_render_test.dart`.
  - [ ] Check the new colours on a device (emulator/phone): splash, themed icon, the
        light vault card, notification tint. Only render tests were run.
  - [x] Token names by role (2026-10-01): `value*`, `destructive*`, `darkCard`,
        `RosePrimitives`; unused gold leftovers deleted; `material_theme.dart` builds the
        Material `ColorScheme` from `AppColors` (AGENTS.md "Palette").
  - *(idea)* N-of-M ring emblem for the avatar (logo study); Quorum Z splash animation.

0. [x] **Proposals expire after ~50 min.** Fixed: vault descriptor `proposal_expiry_blocks` (default 7 days), proposer sets it, members accept it plus ~2 h tip slack; `bridge_e2e` lets 300 blocks pass before approving. PCZTs are built with the library default expiry
   (`DEFAULT_TX_EXPIRY_DELTA` = 40 blocks) and `verify` allows at most 100 blocks, so
   approvals and signing must finish within about 50 minutes. Async multisig needs a long
   expiry (days; configurable per vault, D2). Anchors can go stale safely (V6: witnesses
   can be refreshed after signing, before proving). Blocks any real async use.
1. [x] **One-tap approvals + auto-submit** (spec §9.5.2): approving signs; the member who
   completes the signatures sends automatically (per-proposal setting), or anyone taps
   "Send now". Tested in `one_tap.rs`, `vault.rs`, `bridge_e2e` and on the emulator.
   Follow-ups below. ~~**Auto-submit when the threshold is reached.** The member whose approval completes
   the threshold (known from log order, so exactly one per proposal) starts "collect
   signatures & send" automatically. The manual button stays as the fallback. ~1 day.~~
2. [~] **Notifications** (in progress 2026-09-29): relay pushes on every log append
   (FCM sender done, tested against a mock); app shows local notifications from a
   background check (WorkManager 15 min, FCM wake-up, one-off after backgrounding) and
   opens the payment on tap. FCM is configured (project `zafe-18c4d`) and verified end to
   end on the emulator (relay → Google → device → background check → notification).
   Next: confirm latency on a real phone.
   Open: APNs sender + iOS (NSE). Done: dead FCM tokens are pruned (2026-09-30).
   Done: relay-client TLS (2026-09-30, see M1 "Hosted relay").
3. [x] **Tor** (2026-10-01): "Use Tor" in Settings > Privacy (default off, per device).
   Embedded arti (`zcash_client_backend` `tor` feature), process-wide fail-closed route
   policy (`zafe_core::tor`): lightwalletd and every relay request go through Tor or fail,
   direct connections open at the switch are cut; bootstrap bounded (180 s foreground,
   60 s background checks, which skip the run if Tor isn't up); dormant in the
   background. Home shows "Connecting to Tor…" / "Tor couldn't connect"; typed errors
   end to end. Relay client: reqwest stays for direct, Tor requests use
   `zcash_client_backend`'s HTTP-over-Tor (see AGENTS.md "Tor"). Live test passed
   (bootstrap ~14 s, testnet lightwalletd through Tor). FCM stays direct (content-free).
   Cost: the arm64 Rust library grows 24.7 → 34.7 MB stripped (+4 MB compressed).
   Still to check **on a device**: bootstrap on the phone (cold and warm), fs-mistrust
   on the Android data dir, background check with Tor on (WorkManager and FCM time
   budget), dormant/resume, battery. Follow-ups: pooled relay connections over Tor
   (needs the arti client, which upstream's `Client` doesn't expose), onion endpoints,
   per-vault circuit isolation, iOS backup exclusion of `<appSupport>/tor`.
4. [x] **Incoming history** (2026-09-30): received payments (any transaction paying the
   vault that spends none of its notes, so never change; coinbase included) are read
   from the wallet DB (`VaultWallet::received_payments`, bridge `api/received.rs`), merged
   with proposals by time on Home and `/activity`, open `/received/:txid`, and are
   announced by the background check ("payment received"). Tested: wallet unit test on a
   hand-built DB, `bridge_e2e` (coinbase receipts listed, own spend excluded), Dart tests.
   Checked on the emulator: rows on Home, `/received/:txid` page. Notes: a received payment's sender is
   unknowable (shielded), so there is no "from". Pending (unmined) receipts: see item 6.
6. [x] **Pending incoming payments** (2026-09-30): while the app is in the foreground with
   a synced active vault, a mempool watch (`zafe_core::mempool::watch`, bridge
   `api/mempool.rs` `watch_mempool`, app `providers/mempool_watch_provider.dart`) streams
   lightwalletd's whole mempool (`GetMempoolStream`), trial-decrypts each transaction with
   the vault UFVK and stores the vault's own unmined (`decrypt_and_store_transaction`), so
   an incoming payment is listed as pending before it is mined. Stops when backgrounded or
   on a vault/lightwalletd switch. Tested: Rust unit tests (backoff, cancel), Dart
   `mempool_watch_policy_test.dart`; `bridge_e2e` gained a section (a second 2-of-2 vault
   paid by the first, listed unmined via E's watch, then mined) that **has not been run
   yet**, and nothing was checked on a device. Open: the Home "pending" row wasn't looked
   at on the emulator; background checks don't watch the mempool (only foreground);
   transparent receipts aren't possible (Ironwood-only vaults).
5. [x] **Scan invite QR** on Join (camera, `mobile_scanner`; accepts the raw invite or the
   invite link; permission-denied state with "Open settings"). Invite links
   `zafe://join?invite=...` (app_links; cold and warm start; from inside a vault it's
   "Add vault"), "Share link" on the setup screen, and the setup QR now carries the link.
   Verified on the emulator (2026-09-30): warm-start deep link prefills Join with the
   "Opened from a link" warning, Back → add-vault welcome → ✕ returns to the vault,
   scanner opens with a live preview. Still unverified: cold start, permission-denied state. iOS untested (URL scheme + camera string added).

## M1 — app v1 on testnet (spec §16)

- [x] **Testnet dry run** (2026-09-30, Nothing Phone A024, Android 16, release build,
      public `testnet.zec.rocks`, local relay via `adb reverse`, CLI members B and C with
      `--network test`): join, safety number matched, keygen, first sync (hung once in
      `walletKey`: fixed in `b6fd063`), Valar faucet 0.125 TAZ landed in Ironwood,
      spendable after 10 confirmations; B proposed 0.01 TAZ with the phone locked:
      FCM push → background check → notification in ~5 s (no ANR in release); C approved
      one-tap; the phone approved with the unlock prompt, completed the signatures and
      auto-sent (proved on the phone): txid e6ccd6b7…0237 mined at ~4422297, vault
      12500000 → 11490000 zat (payment + 10000 fee). Still to check on the phone: live
      camera QR scan, haptics, proving time from logs (adb dropped before they were read)

Done
- [x] Flutter + FRB app on Vizor's architecture and design system (`c07c687`)
- [x] Create / join (paste) / seal / safety number / keygen, receive, balance (`c07c687`)
- [x] Payment proposals, independent check on device, approve/reject, async signing
      through the relay, send with progress (`c508aff`)
- [x] Settings, app-wide hide amounts, theme (`c508aff`)

Open
- [x] Auto-submit (see Next up 1)
- [~] Push notifications (see Next up 2)
- [x] Clients speak TLS (2026-09-30): relay client (reqwest `rustls-tls`) and lightwalletd
      (tonic `tls-ring` + `tls-webpki-roots`, explicit `ClientTlsConfig` for `https`), both
      with the bundled Mozilla roots; `crates/zafe-core/tests/tls.rs` (local rustls server:
      trusted cert works, untrusted and wrong-hostname refused; ignored live test against
      `testnet.zec.rocks:443` passed). App `ZAFE_NETWORK=test` preset (TLS lightwalletd,
      placeholder relay URL). `cargo ndk` check for arm64 passes with ring
- [x] Relay packaging (2026-09-30): `infra/relay/Dockerfile` (non-root, `/data` volume,
      `$PORT`), `GET /health`, SIGTERM shutdown, FCM key from a file or secret env var;
      Fly.io template, VPS recipe (systemd + Caddy), nightly backup timer, README
- [x] Hosted relay deployment (testnet), 2026-10-06: OVH VPS-1 (UK, Ubuntu 26.04,
      57.129.172.198) at `testnet.relay.zafe.cash`, Docker Compose + Caddy, deployed by
      `.github/workflows/relay-deploy.yml` (GHCR image by digest, provenance, health check,
      rollback; `infra/relay/README.md` Path B). Live: first CI deploy green (`abc19c0`),
      `https://testnet.relay.zafe.cash/health` answers ok, release variable `ZAFE_RELAY_URL`
      set. Left: **(you)** make the GHCR package `zafe-relay` public (self-hosters, and
      `gh attestation verify` without login), then a testnet build against it.
      Relay is SQLite today, Postgres for the hosted tier
- [x] Relay rate and size limits (2026-09-30): token buckets per signing key (after the
      signature verifies) and per client IP (`zafe_relay::limits`, 429 + `Retry-After`,
      env-tunable, proxy header for Fly/Caddy), 1 MiB body cap; client
      `RelayClientError::RateLimited` → `NotReady` ("the relay is busy"). Keygen
      (`node_keygen`) and the whole payment flow (`bridge_e2e`) run under the hosted limits
- [x] Live activity while the app is open (2026-09-30): relay long poll `POST /v1/wait`
      (up to 25 s, woken by log appends and deliveries, 2 waits per key / 4096 total),
      client `wait_for_activity`, bridge `watch_vault` stream, Home refreshes proposals on
      each event and relaxes its poll to 60 s while live. Fallback: a relay without the
      endpoint (404) or a failing watch leaves the 15 s poll as before; background stays
      on FCM + WorkManager. Tests: `zafe-relay/tests/wait.rs`,
      `zafe-core/tests/relay_wait.rs`, `app/rust/tests/vault_watch.rs`,
      `app/test/live_vault_watch_test.dart`. Not yet checked on a device. *(idea)* the
      leader's share collection (inbox polled twice a second) could use the same wait
- [ ] Relay follow-ups: off-site backups (Litestream) *(idea)*. Done: dead FCM tokens
      (404 `UNREGISTERED`) are deleted (`FcmNotifier::on_unregistered` →
      `Relay::forget_push_token`); storage quotas (see below); a cap on mailboxes created
      (2026-10-01): 20/day per client IP (`Limits::creates_per_ip`, 429) and 8 per signing
      key (`Quotas::mailboxes_per_key`, 507, counted on an index of `mailboxes.creator`);
      env `ZAFE_RELAY_CREATES_PER_DAY` / `ZAFE_RELAY_MAX_VAULTS_PER_KEY`
- [x] Invite by link: `zafe://join?invite=...` custom scheme (see Next up 5)
- [x] Universal/App Links (`https://…/join#invite`) so a link works for people without
      Zafe installed (2026-10-01): build-time `ZAFE_LINK_HOST` (unset → invites stay
      `zafe://`); invite in the fragment; Android `autoVerify` filter with the host from
      the dart-define (Gradle placeholder); landing site `infra/site/` (`/join` page, CSP,
      no third-party loads, invite moved to `sessionStorage` and out of the address bar;
      `build.sh` writes `assetlinks.json` + AASA); release workflow variable. Tested: Dart
      unit tests, merged manifest (host set / unset / malformed), page rendered in a
      browser (light, dark, no invite, reload)
  - [x] Landing page (2026-10-01): `infra/site` moved to Astro; `/` explains Zafe (hero
        with the app's Home screen, on-chain comparison with Safe, how it works, security
        features, honest caveats: testnet, no audit yet, members see everything, keep
        backups); no JS on it. Checked in a browser at 1280/390 px, light and dark
  - [x] Landing page redesign (2026-10-01, user: "looks too much AI slop"): researched
        (24 reference pages, audience, craft; `docs/site.md`), rebuilt as direction "The
        approval": the approval screen as the hero, the real dry-run payment as members vs
        chain see it, the payment path, the on-device check, key shares, a dense feature
        list, limits, sources. Checked at 1440/390 px, light and dark
  - [x] Second round (2026-10-01, user: "nah, sucks. take inspiration from vizor.cash"):
        rebuilt on Vizor's structure (centred hero, three members' phones on a stage,
        illustrated pillars, dark feature panel with HTML UI cards, FAQ, closing card,
        ghost-wordmark footer) in Zafe's type, colours and art (`docs/site.md`)
  - [x] Third round (2026-10-01): light only; CSS logo intro (the tile fills the
        screen, pieces lock, it ripples up into the nav logo); much shorter copy
  - *(idea)* Show a second real payment (with a memo) once one is made; add a changelog
        or "last updated" to the site
  - [x] Domain: **zafe.cash** (Spaceship, registered 2026-10-01, expires 2027-10-01;
        move to Cloudflare Registrar after the 60-day lock for the cheaper renewal);
        `site` points at it
  - [x] zafe.cash serves the Vercel project (www → apex); Cloudflare Email Routing
        forwards every `@zafe.cash` address; `/.well-known/security.txt`
  - [x] Release upload key made 2026-10-01 (RSA 4096, alias `zafe`, cert SHA-256
        `D8:A2:F2:C1:…:F6:9C`); repository variables `ZAFE_ANDROID_CERT_SHA256` and
        `ZAFE_LINK_HOST=zafe.cash` set; zafe.cash serves `assetlinks.json` (Google's
        statements API returns it)
  - [ ] **(you)** Back up the upload key and its password in a password manager, set
        the four `ANDROID_*` signing secrets (`docs/releasing.md`), and add Play's
        app-signing fingerprint to `ZAFE_ANDROID_CERT_SHA256` if the app goes on Play
  - [x] SEO pass (2026-10-01, `docs/seo.md`): canonical/OG/Twitter tags, JSON-LD,
        sitemap.xml + robots.txt from `site`, favicons + manifest
  - [ ] SEO follow-ups (`docs/seo.md` "Open"): change `site` with the domain and add it
        to Search Console. Done 2026-10-01: home LCP (was none; now 2.0 s, Lighthouse
        99/100/100), /showcase CLS 0.384 → 0, a 404 page; the story heading, contrast and
        og.png were already fixed. /showcase still reports no LCP (its fade-in entrance)
  - [x] Checked 2026-10-01 on the emulator (release APK signed with the upload key,
        `ZAFE_LINK_HOST=zafe.cash`): `pm get-app-links` says `zafe.cash: verified`;
        https and `zafe://` invite links open Join cold and warm (screenshots of Join are
        black: it's a `SecureScreen`, FLAG_SECURE). Not yet checked: with Zafe
        uninstalled the link opens the page
  - [ ] iOS: Associated Domains entitlement (`applinks:<host>`) and the team's app ID in
        the AASA, once iOS builds
- [ ] iOS: `permission_handler` needs `PERMISSION_CAMERA=1` in the Podfile
      `GCC_PREPROCESSOR_DEFINITIONS` if we ever request through it (today only
      `openAppSettings` is used; `mobile_scanner` asks for the camera itself)
- [x] Cancel a proposal (author) in the UI (2026-09-30): "Cancel payment" on the proposal
      page with a confirmation that warns when every signature is already in (it stays
      sendable until expiry); bridge `cancel_proposal` → `node::cancel`. Tested in `bridge_e2e`
- [x] Proposal expiry in the UI (2026-09-30): `ProposalInfo.expiry_height`; "Expires in
      about N days" on open/approved proposals; once the synced tip reaches it, rows, chip
      and page say "Expired", it stops counting as needing action and isn't auto-sent;
      "Propose again" opens Send on the review step prefilled (`SendPrefill`, single
      payment). Dart tests in `test/proposal_expiry_test.dart`. Emulator (2026-09-30): the
      "Expires in about 7 days" line; after mining past expiry the row, chip, title and
      page read "Expired" (amount struck through), and "Propose again" opened the review
      step prefilled and logged a new proposal. The CLI's `proposals` still prints an
      expired one as `Open` (no tip there)
- [x] Expiry window chosen at vault creation (D2; 2026-09-30): "Approval window" 1/3/7/14
      days on Create; the creator's DKG round-1 message carries it (`DKG_ROUND1` = 2, no
      invite change, so backups are unaffected) and every member signs it in the
      descriptor; 1..30 days accepted. CLI `vault keygen --expiry-days`. Not shown in
      Settings yet
- [x] Expiry privacy (2026-09-30): expiry heights are rounded up to a multiple of 144
      blocks (`vault::expiry_height`), so they don't date the proposal; members accept
      window + 96 + 144. The long window itself still sets vault transactions apart from
      40-block wallets: inherent to approving over days (a small fixed menu keeps vaults
      in a few buckets)
- [x] "Start over" for a signing round (2026-09-30): after a timeout the leader can "Start
      over with other signers" (bridge `restart_signing` drops `<id>.req`/`.own`; used
      commitment sets stay used); members whose approval's nonces are gone
      (`ProposalInfo.needs_reapproval`: used in the unfinished round, or restored from a
      backup) see "Approve again". Tested in `bridge_e2e` (round 1 times out, start over,
      A re-approves, C joins, sent). Emulator walkthrough (2026-09-30, app + CLI B/C):
      round with C timed out, start over reported "0 of 2 signers are ready", B approved,
      the app showed "Approve again", the new round with B was sent. It found and fixed:
      the proposal page kept the failed-send card with only "Try again" and never offered
      "Approve again"; the sending screen had no "Start over". Cancel checked too
- [x] Start-over follow-ups. Done: members get a one-time "approve a payment again"
      notification (`reapprovalKey` in the seen snapshot) and it counts as needing action.
      Left as is (2026-10-01): the unresponsive signer keeps nonces for commitments no
      leader will use; they're never reused, and `node::forget_closed_nonces` deletes
      them once the proposal is closed or past its expiry height, which every proposal has
- [x] Note reservation across concurrent proposals (`reservedNotes`, spec §9.1; done
      2026-09-30): before building, `node::propose` replays the log and locks, in this
      member's wallet, every note an open/approved/broadcast proposal (or a cancelled one
      with a complete group) spends, until that transaction's expiry
      (`node::note_holds` → `VaultWallet::reserve`, upstream `OutputLockStore`, owner =
      PCZT hash). Works across members, not just on one device. Short of unheld funds →
      `WalletError::FundsReserved` / `ZafeErrorKind::FundsReserved`. Tested in `regtest_e2e`
- [x] Reservation follow-ups (done 2026-09-30): (a) **replay rule**: a `Proposal` that
      spends a note of an earlier open/approved proposal not yet expired (as of the new
      proposal's `tip_height`) is ignored by every member (`VaultError::NotesInUse`);
      `node::propose` sees it at append time and rebuilds from the remaining notes (3 tries);
      (b) `sync_vault` refreshes holds after every sync (`node::reserve_notes`), so every
      member's spendable balance leaves out held notes; (c) a broadcast tx that isn't mined
      and isn't in lightwalletd's mempool any more releases its notes (the whole mempool is
      read, `wallet::mempool_txids`, so lightwalletd doesn't learn the txid). Cancelled
      proposals hold nothing now, so a new proposal can respend (invalidate) their notes
- [x] Rebroadcast a dropped transaction (2026-09-30): the broadcaster keeps the raw tx
      (`node::SentTxs`, `<db_dir>/sent-<vault>/<txid>.tx`, CLI `<home>/sent`) until it is
      mined or expires; `reserve_notes` resends it when it's neither mined nor in the
      mempool (and releases the notes only if the node refuses it or no bytes exist). It
      now acts only when lightwalletd's tip **equals** the wallet's (a restarting node
      reports 0). `bridge_e2e` checks the file is kept and deleted once mined. The resend
      itself has no automated test: Zakura keeps its last ~100 blocks in memory, so a node
      restart rewinds the chain too and can't simulate a dropped transaction
- [ ] Other members can't resend (only the broadcaster has signatures + proof); one-tap
      proposals could be rebuilt by anyone from the log *(idea)*
- [x] Member names (local labels, 2026-09-30): tap a signer on Home to name them; the
      name (`<vaultDir>/names.json`, `memberNamesProvider`) shows in signer rows (short
      key below) and as "Proposed by". Local to this device: not synced, not in backups,
      not in notifications yet. Checked on the emulator
- [x] Member names follow-ups (2026-09-30): names are in encrypted backups (`BACKUP` v2,
      v1 still restores with no names) and restored with the vault; notifications name
      a named proposer / rejecters / canceller (unnamed signers aren't mentioned); the
      CSV's proposer and approvers columns read "Name (hexkey)" (approvers now `; `-
      separated). Not on a device yet
- [x] Share signer names via the log (2026-10-01): each member names themselves
      (`VaultEvent::Name`, event version 2; "Your name" on their own Signers row, CLI
      `zafe name`); everyone sees it in signer rows, proposals, notifications and the CSV,
      with this phone's own label taking precedence. Tests: `tests/vault.rs`
      (`members_share_their_names`, `version_1_events_still_replay`), `node_keygen`
      (over the relay), `member_names_test.dart`. Needs an on-device check
- [x] Endpoint settings editable (done 2026-09-30): Settings → Relay / Zcash server opens
      a sheet (format check: https except localhost/127.0.0.1/10.0.2.2 on regtest; then a
      live test: relay `GET /health`, lightwalletd `GetLightdInfo` incl. network) and
      "Reset to default". Saved in prefs per network (`zafe_relay_url_<net>`), read in the
      bootstrap and by background checks; a relay change re-registers the push token.
      The network stays compile-time. Needs an on-device check
- [x] Endpoint settings before the first vault (2026-10-01): a cog on the welcome screen
      opens "Server settings" (Relay, Zcash server → the same editor as Settings); with
      the placeholder relay the welcome screen says "No relay set. Add one". Test:
      `test/welcome_server_settings_test.dart`; previews: `tool/screens/welcome_render_test.dart`.
      Needs an on-device check
- [ ] iOS: build and run at all (only Android has been exercised)
- [ ] iOS: exclude the nonce directory from backups (`isExcludedFromBackup`)
- [x] Biometric/passcode gate before approving and signing (spec §14; done 2026-09-30,
      `local_auth`; approve, send, propose, backup export, vault removal; setting in
      Settings, default on). Needs an on-device check with a real screen lock
- [x] Payment sounds + haptics (2026-10-06, `docs/sounds.md`): approve, ready, sent,
      received, failed; "Payment sounds" in Settings
  - [ ] iOS handler for `xyz.zafe/payment_feedback` (AVAudioPlayer, CAF/M4A copies; the
        ringer switch should silence it)
  - [ ] Hear them on a real phone (the emulator runs with `-no-audio`)
- [x] First payment is one tap (2026-10-06): every member publishes its commitment pool at
      the end of keygen (bridge `run_keygen` + CLI), not on its first refresh
- [ ] One-tap needs **every** member's pool (`vault::assign_commitments`), not just t of
      them: a member who never opens the app again stops one-tap for the whole vault once
      the keygen pool (16 proposals' worth) is used up. Assign only groups whose members
      all have pools. Changes replay, so gate it like an event version; needs a spec note
- [x] Balance card: one total + dollar value (mainnet only), no pending notes (2026-10-06)
- [x] Play in-app updates (2026-10-06): background download + "Update ready" card;
      priority ≥ 4 takes over the screen
  - [ ] Test end to end through Play (internal app sharing) once the app is on Play
  - [ ] iOS: an update prompt from the App Store version (no Play equivalent)
  - [ ] Releases: set the in-app update priority per release (Play Developer API
        `inAppUpdatePriority`; the Console has no field), e.g. in `release.yml`
- [ ] Dollar values beyond the card (payment amounts, send review) *(idea)*
- [ ] Unlock gate follow-ups: it is a UI gate only (key material in secure storage is not
      bound to user authentication; a Keystore key with `setUserAuthenticationRequired` /
      Keychain `.userPresence` would make it cryptographic, but background round-2 signing
      needs the key while locked, spec V8). Done 2026-10-01: "Open settings" in the
      no-screen-lock note (`xyz.zafe/system_settings` → `ACTION_SECURITY_SETTINGS`, falls
      back to Settings; the note re-checks on resume); needs an on-device check.
      Done 2026-09-30: Launch/NormalTheme use AppCompat parents
      (+ explicit `androidx.appcompat`), so the biometric prompt can't crash on Android 8
      and below; checked the app still launches (emulator, API 35)
- [x] SQLCipher for the wallet DB (spec §14; done 2026-09-30): per-vault random key in
      secure storage, `PRAGMA key` on every connection, unreadable/plain DBs are deleted
      and resynced. Vendored OpenSSL (static libcrypto) on every target
- [ ] Wallet DB follow-ups: iOS could use CommonCrypto instead of vendored OpenSSL
      (smaller binary; target-specific rusqlite features); the key lives in the same secure
      storage as the vault material, so it protects against file-level copies (backups,
      forensic dumps of app data), not against an attacker who can read the Keystore
- [x] Pre-warm the proving key when a proposal becomes Approved (2026-09-30): bridge
      `prewarm_prover` (proving + verifying key `OnceLock`s), started once per process by
      `ProposalsNotifier` when any unexpired proposal is approved. Costs ~113 MB for the
      process lifetime on a phone; only on devices that see an approved payment
- [ ] Low-end device benchmark (V7 still open: Cortex-A55-class phone)
- [x] App icon, launcher name/branding: original vault-dial icon (adaptive + themed
      monochrome, legacy mipmaps, iOS AppIcon), label "Zafe", native splash in the window
      colour (Android 12+ and older, iOS LaunchScreen). Regenerate: `scripts/brand/icons.sh`.
      Needs a look on a real launcher (Android and iOS) once a device build is made
- [x] Release build + signing + GitHub Releases (2026-09-30): `.github/workflows/release.yml`
      (tag `v*` or manual) builds a signed arm64 testnet APK (`--split-per-abi`: plugins
      ship extra ABIs our Rust lib lacks, and Flutter's Gradle plugin overwrites buildType
      abiFilters) and publishes a pre-release with SHA-256 + certificate fingerprint;
      refuses without `ZAFE_RELAY_URL` or signing secrets. Gradle signs from
      `android/key.properties`. Setup: `docs/releasing.md`. Local release builds checked:
      arm64 61.8 MB (3 ABIs before the split), x86_64 split 57.8 MB launched on the
      emulator, created a vault and shared the invite QR (R8 kept every plugin working).
      **(you)** keystore, secrets, `ZAFE_RELAY_URL`; the workflow has not run on GitHub yet
- [ ] Reproducible / F-Droid builds (Vizor `scripts/build-android-reproducible.sh`) *(idea)*
- [x] Tabs (2026-09-30, requested: Home was too cluttered): Home, Activity, Signers,
      Settings under Vizor's floating tab bar (`StatefulShellRoute`). Home keeps the
      balance, actions, notices and 4 recent rows; the signers card moved to its own tab
      (room for key rotation and other signer management later).
- [x] Rename a vault (2026-09-30): local to this device (like signer names), from
      Settings → Name. Open: carry it in backups; a vault-wide rename would need a new
      signed log event (`VAULT_EVENT` bump, gated on every member updating) *(idea)*
- [x] Relay forgets handled deliveries (2026-10-01): `/v1/inbox/ack`, sent by
      `node::respond` (see AGENTS.md "Inbox acknowledgement"); tested in the relay's
      `quotas` tests and `node_keygen` (keygen messages gone after the first respond).
      Not yet: the log is never pruned (old commitment batches could be compacted);
      entry headers still show the author key to the relay *(idea)*
- [x] Cached switcher balance moved from `summary.json` to secure storage (2026-10-01)
- [x] Vault emblem instead of "2/3" on Home and in the switcher (2026-10-01)
- [ ] App shows under "Other" in the Nothing launcher: Android's `appCategory` has no
      finance value; expect it to follow the Play Store category once listed *(check)*

## Multiple vaults, import and export (requested 2026-09-30)

Multiple vaults on one device (done 2026-09-30, verified on the emulator: migration from the
single-vault layout, add via the switcher, cross-vault notification tap, remove)
- [x] **Storage per vault**: secure store keyed by vault id (today one identity, invite and
      material slot); per-vault `state_dir` (`used_commitments.bin` and the notification
      snapshot are shared today); wallet DBs are already per vault (`vault-<id>.sqlite`)
- [x] **A fresh member identity per vault** by default, so the relay can't link one
      person's memberships across vaults (it sees each member's public key)
- [x] **Vault switcher**: Vizor's account sheet pattern (tap the top-nav vault name/avatar),
      showing each vault's name, rule (2/3), balance (respecting hide amounts) and a badge
      for payments needing approval; "Add vault" (create or join) from the sheet
- [x] Per-vault settings; **remove vault from this device** with a clear warning (it
      doesn't leave the vault; say what the other members lose if this was needed for t)
- [x] Background checks, notifications and push registration for **every** vault
      (notification title already carries the vault name)
- [ ] Revisit the bridge's single global wallet lock (Known issues) for parallel syncs

Export and import (done 2026-09-30, verified on the emulator: export with a suggested
passphrase and share sheet, restore of another member's seat from pasted text, wrong
passphrase and already-on-this-phone refusals, backup prompt after creating a vault)
- [x] **Export this vault** as an encrypted file (spec §12.2: identity, FROST key package,
      `sk`, descriptor, log key, `use_qsk`; **never** nonces or pool nonces; Argon2id ≥
      64 MiB + XChaCha20-Poly1305; strong passphrase), via the share sheet or saved file.
      Versioned format with a magic header
- [x] **Import a vault** on a new or second device from that file + passphrase, then
      resync the log and the wallet from the birthday; publishes a fresh commitment pool
- [x] Decide **move vs. copy** semantics (decided: backups are passive copies; "move" = restore on the new phone, then Remove on the old one, as the export screen says): two live devices holding the same member share
      can both vote and sign (fresh nonces each, so no key leak, but confusing votes).
      Proposal: "move to a new phone" exports, then retires this device's copy
      (deletes material and pool nonces after the import is confirmed); plain backups
      stay passive files
- [x] **Viewing-key export** for auditors (2026-09-30): Settings → "Viewing key" (unlock
      first) → `/viewing-key` (SecureScreen): the UFVK (`vault_viewing_key`, derived from
      the material and checked against the signed descriptor) as text, QR and copy. Not
      on a device yet
- [ ] **Import a vault as view-only** (auditor mode: balance and history from a UFVK, no
      signing)
- [x] Backup health: per-device prompt after creation + home reminder; members attest backups in the log (`BackupVerified`), the Signers tab shows who has one (2026-10-01)
- [ ] Multi-part QR for device-to-device transfer (material is a few KB) as an
      alternative to files, *(idea)*

## M2 — v1 feature-complete, mainnet beta (spec §16)

- [x] Batch payments UI (2026-09-30): "Add another recipient" on the review step (up to
      50), a recipients list with remove, available balance net of added recipients;
      proposal pages list every recipient (`RecipientsCard`); "Propose again" keeps the
      batch. Emulator: a 2-recipient proposal verified on B (fee 15000 zat, 3 actions),
      approved, signed interactively with B and sent
- [x] Import batch recipients from CSV (2026-09-30): "Import recipients from CSV" on the
      recipient step, "Import CSV" on review; `address,amount[,memo]`, optional header,
      ZEC decimals, RFC 4180 quoting (`features/send/recipients_csv.dart`, unit-tested);
      all-or-nothing with row-numbered errors; respects the 50 cap. Not on a device yet
- [ ] Batch follow-ups: per-recipient edit (today: remove and re-add)
- [ ] Address book via `ADDRESS_BOOK` proposals (t approvals, no FROST); warn on unknown
      recipients (§11.2)
- [ ] Rules via `RULES` proposals: per-tx / per-period limits, allowlist-only,
      large-payment extra approvals; enforced by apps before voting and signing (§11.1)
- [x] CSV export of vault history, generated locally (§11.3; 2026-09-30):
      `zafe_core::history` (sent rows from the log, one per payment, fee on the first;
      received rows from the wallet; UTC ISO dates, exact 8-decimal ZEC, RFC 4180 quoting,
      formula injection defused for memos), bridge `export_history_csv`, "Export" on
      Activity (unlock, then share sheet). Tested in `bridge_e2e` and on the emulator.
      Contact names stay empty until the address book exists
- [x] Encrypted backup (Argon2id + XChaCha20-Poly1305; file via the share sheet, or text) and restore;
      never includes nonces (§12.2–12.3)
- [x] Backup health: members attest a backup that opens in the log; Signers tab shows
      it (§12.2; 2026-10-01). Not yet: a later "check my backup" (re-enter the passphrase)
- [x] Repair a lost device's share (2026-10-01; §10.1, §10.4.2): seat moves with t
      approvals in the log and on the relay, RTS repair by the first t approvers, the new
      phone checks the share against the public key package, the group key and the UFVK.
      Follow-ups done 2026-10-01: a member can take over from a stalled helper (new
      attempt), QR scanning of the recovery code, notifications for pending moves,
      background checks keep the membership current. Left: a device run of these three
- [ ] Transparent / TEX recipients with a warning (spec allows; verify.rs is
      Ironwood-only today)
- [ ] External security audit of zafe-core and the protocol
- [x] Mainnet gate U1: answered by ZF on frost#1094 (2026-10-01)
- [ ] **(you)** Mainnet musts: the audit above, a hosted relay (OVH VPS) with its URL in
      `kMainnetRelayUrl`, and a small end-to-end mainnet dry run (create, receive,
      propose, sign, send, restore) incl. `zec.rocks:443` with the app
- [x] Relay rollback check (2026-10-07, core + relay + docs on main; the app side is on
      branch `audit-prep-app`, see below): every device keeps a copy of each vault's log
      (`log_cache`), `load_log` refuses a relay without its last entry
      (`RelayRolledBack`/`RelayForked`/`RelayLostVault`); `POST /v1/mailbox/reseed` +
      `node::reseed_relay` + `zafe restore-relay` put a lost vault back on a relay from a
      member's copy (kept; moving between relays stays dropped, see the decision below).
      Tests: `relay_rollback`, `log_cache`.
- [x] App side of the above (2026-10-07, merged from `audit-prep-app`, codegen re-run):
      bridge (`init_log_cache`, `restore_relay`, `unapproved_spends`, error kinds), Dart
      copy, sync-sheet "Restore vault on the relay" (same relay only; no copy implies
      moving a vault), beta label (`core/config/beta.dart`), unapproved-spend card +
      notification, `kMainnetRelayUrl`. Dart tests: error copy, sync failure kinds,
      unapproved-spend notification, beta. `bridge_e2e` keeps the interactive fallback
      by letting only A publish a pool (no group fully covered) and asserts `!one_tap`.
- [x] Audit section 9 follow-ups (2026-10-07, `VAULT_EVENT` 7): broadcast txid checked at
      replay, relay membership/threshold check after a restore, "Follow the relay" for a
      forked device, grace limited to approved proposals, Litestream/Caddy pinned by digest.
      See `docs/audit-scope.md` section 10. Still open from section 9: beta slots, same
      SSH key for testnet/mainnet, `catch_relay_up` across seat moves.
- [x] One-tap pools no longer need every member (2026-10-07, `VAULT_EVENT` 6, gated by
      the author's event version; `vault` tests)
- [x] Unapproved-spend alert core (`spend_watch`, `VaultWallet::vault_spends`; wallet
      query checked on regtest in `regtest_e2e`, and `bridge_e2e` asserts nothing is
      flagged once the log accounts for the spend)
- [ ] **(you)** Mainnet relay one-time setup (files prepared, nothing deployed): DNS
      `relay.zafe.cash`, S3 bucket + keys, GitHub environment `relay-mainnet` with required
      reviewers and the variables/secrets in `infra/relay/README.md` "Mainnet"; first
      promotion from `relay-mainnet.yml`; push-triggered testnet redeploy will use the new
      `deploy.sh <network>` signature (check the first run)
- [ ] **(you)** Audit: `docs/audit-scope.md`
- [x] (old note) Relay rollback check, before mainnet (found 2026-10-06): apps keep no saved head
      (`node::load_log` re-reads from entry 0), so a relay serving an older log (a restored
      backup, or a self-hosted relay run by someone else) is accepted silently and can bring
      back a cancelled proposal that already has t one-tap shares. Fix: store the log length
      + head hash per vault in secure storage and refuse a shorter/forked log (spec §6.3
      "relay inconsistency")
- [x] **Decided 2026-10-06: one relay per vault, no relay migration.** Considered and
      dropped: moving a vault between relays (fallback lists, signed move entries, history
      re-seeded from a device log cache) and per-member home relays. Scaling = stateless
      relay instances on a shared remote DB (Postgres; managed backups cover DB loss).
      Self-hosting = a DAO runs `infra/relay/vps/compose.yml` and all its members connect to
      it. Follow-up: spec §6/§14 still promise moving relays and a device log cache; update
      them when the spec is next revised
- [x] Zcash server list + failover (2026-10-06): public lightwalletd presets per network
      with latency in Settings, custom URL, automatic failover between listed servers
      (AGENTS.md "Zcash server list"). Not done: switching back to the chosen server when
      it recovers, failover in background checks
- [ ] Relay URL per vault: the invite carries the vault's relay URL and the app stores it
      with the vault (today one relay setting per network for the whole app), so vaults on
      a self-hosted relay and on ours coexist in one app. Bumps the invite version
- [ ] Mainnet relay: second compose service + `relay.zafe.cash` + Litestream to object
      storage + a `relay-mainnet` environment with required reviewers that promotes a
      digest already on testnet
- [ ] Deploy-key split (files prepared 2026-10-07): the user runs the migration in
      `infra/relay/README.md` ("Migrating a running server"); then delete the legacy
      `RELAY_SSH_KEY` fallback in `relay-deploy.yml`. Consider a separate host for mainnet.
- [x] Mainnet beta: "beta" label only, no cap (decided 2026-10-07: no relay vault limit, no
      per-vault fund limit)

## M3 / M4 — later (spec §16)

- [ ] Rotation, same address and t (§10.2): add members by repair toward a new identifier
      (§10.4.2; checked 2026-10-04 on our pins: 2-of-3 → 2-of-4 signs once members add the new
      verifying share), remove members by DKG refresh (§10.4.1)
- [ ] Change t on the same key by resharing (spec §10.4.4): own code (not in frost-core,
      frost#1082), spec + external audit first; reference test `zafe-core/tests/reshare.rs`
      (2-of-3 → 3-of-4 → 2-of-3, dishonest dealers caught). Rotation for friendly changes
      only: kept old shares still sign at the old t, nothing can prevent that
- [ ] One-time ceremony encryption keys (spec §10.4.4): forward secrecy for DKG, refresh,
      reshare and repair messages (today sealed to long-term identity keys); must ship
      before refresh/reshare. Later maybe signing rounds (they carry `alpha`)
- [ ] Migration to a new vault (hostile removal, change t until resharing ships) (§10.3),
      optionally resharing the old key to the new set so it can sweep late payments to
      the old address
- [ ] Unapproved-spend alert (spec §10.4.4): a vault spend matching no proposal in the log
      → loud alert on every member's phone (old shares or compromised devices used)
- [ ] Desktop builds (Vizor has desktop layouts to copy)
- [ ] Self-hostable relay packaging; paid hosted tiers (D3 limits)
- [ ] dApp SDK: proposal requests from third-party apps
- [ ] Hardware members (Keystone/Ledger) if they gain FROST support
- [ ] Payment disclosures (ZIP 311-style proofs for auditors)

## One-tap follow-ups

- [x] **Cancel after completion** (2026-09-30): a cancelled proposal whose signatures
      are complete (`ProposalInfo.still_sendable`) offers "Make it unsendable":
      `node::invalidate` proposes a **sweep** (a `Proposal` with no payments, auto-send)
      spending exactly its vault notes (padding spends excluded) to the vault's internal
      address (`VaultWallet::propose_sweep`: every other note is locked for the call, then
      `propose_send_max_transfer`). Members' verification accepts it unchanged (all
      outputs are the vault's). The cancelled page links to it (`invalidated_by`); rows show
      "To vault"; `SweepCard` on its page. Tested in `bridge_e2e` (invalidate, review,
      approve, send)
- [x] Delete pool nonces of **expired** proposals (2026-09-30): `forget_closed` takes the
      synced tip (bridge `list_proposals(tip_height)`, passed by the app and background
      check); tested in `tests/vault.rs`. Interactive nonces of closed or expired
      proposals are deleted too (`node::forget_closed_nonces`, same refresh)
- [x] Surface "one-tap unavailable" (2026-09-30): the Approve step explains that approvers
      sign again when it's sent. Pools: the app tops up on every proposals refresh (first
      Home refresh after keygen); the CLI only with `zafe pool`, so harness vaults are
      interactive unless each CLI member runs it
- [ ] A member whose device lost its pool nonces approves interactively (fallback works);
      show that their approval still needs a signing round
- [ ] Push notifications now matter less (approvals are final at tap), but still needed
      so members learn about new proposals
- [x] CLI: `approve` doesn't auto-send (by design for scripts); its help and its
      "signatures complete" line now point to `zafe send` (2026-10-01)

## Upstream and waiting on others

- [x] ZF questions (U5, U1, U3) posted as one comment on frost#1094 (2026-10-01):
      https://github.com/ZcashFoundation/frost/issues/1094#issuecomment-5928093098
- [ ] **(you)** Post the Discord pointer in `#frost` (`upstream-asks.md`); follow up on answers
- [x] U1: answered on frost#1094 (2026-10-01): change "very unlikely"; our own `sk`
      agreement works
- [ ] Get `test-vectors/zip2005_use_qsk.json` cross-checked upstream: issue
      frost-tools#609 + PR frost-tools#610 (2026-10-04, from the `zafe-cash/frost-tools`
      fork): fixture + a Rust test that recomputes it from the ZIP; awaiting review
- [x] U2: answered on frost#1094 (2026-09-21): an external-randomizer API will always
      exist (`sign()` may be un-deprecated). Keep the one `#[allow(deprecated)]` wrapper
- [x] U3: answered (2026-10-01): moves to the FROST repo, same serialization. Switch
      crates when the move ships (frost#963)
- [ ] U4: COCKTAIL-DKG Pallas (frost#1033; WIP frost#1032 has no Pallas). The ZIP 312
      keygen spec exists in the zips#895 draft ("Contributory Generation of sk"): align
      Zafe's `sk` agreement with it when COCKTAIL-DKG is usable
- [ ] FROST book `book/src/zcash/technical-details.md` still says to throw `sk` away and
      that the key share is the only secret (against ZIP 2005): send a docs PR
- [x] U5 security: answered 2026-10-01 (Daira-Emma Hopwood on Discord, quoted in
      `upstream-asks.md`): the proof doesn't need `alpha` after the commitments; `alpha`
      links `rk` to `ak`, so it stays among members (spec §9.5.1)
- [x] Sent Daira the Discord follow-up (2026-10-01); she answered the same day: both fine,
      `use_qsk = true` with `ak` from the DKG is the intended usage (`upstream-asks.md`)
- [ ] **(you)** Answer Daira's question on how ZIP 2005 could be clearer (draft in chat,
      2026-10-01); offer a zips PR if she wants one
- [x] U5 conformance: ZF accepts the ZIP 312 deviation (conradoplg, 2026-10-01)
- [x] thus-spoke-zakura PR #124 merged 2026-10-01, released in v0.3.0 (2026-10-02); `infra/regtest` pins its images. Was: (Ironwood fix, targets `main`; replaces #119, auto-closed when `dev` was deleted on 2026-09-30): rebased onto `main` aee7880 2026-10-01, all CI steps + all 3 Docker recovery tests pass locally; CI waits on maintainer approval. Follow up until merged
- [x] Regtest runs through `ths` itself (2026-10-04): `infra/regtest/up.sh` starts `ths start`
      in the background, vaults are funded from the faucet (`fund.sh`), mined with `ths mine`;
      `regtest_e2e`, `bridge_e2e` and `m0-e2e.sh` pass (faster: bridge 98 s → 40 s). Lost:
      coverage of coinbase receipts (`is_coinbase`), since ths mines to its own wallet
- [ ] `ths stop` leaves a foreground `ths start` running (0.3.0): issue zcashlabs/thus-spoke-zakura#145,
      fix PR #146 (2026-10-04). Follow up until merged; then `down.sh` could drop the SIGINT
- [ ] Proposed `ths start --detach` for scripts/CI (thus-spoke-zakura#147, 2026-10-04): opt-in,
      foreground stays the default (their #14). If accepted, send the PR; then `up.sh` becomes
      `ths start --detach` and `down.sh` `ths stop`
- [ ] **ZIP 321 link security** (zodl-project#60, open since 2024, `I-SECURITY`): proposal
      posted 2026-10-04
      (https://github.com/zodl-inc/zodl-project/issues/60#issuecomment-5982699354):
      (1) a verified https link per wallet (`https://<wallet domain>/zip321#<zcash: URI>`, no
      shared `pay.z.cash`, answers nuttycom's centralisation/IP objections) and (2) a check
      code per request (memory-hard hash, ~30 bits, number matching) against tampering on
      any channel; complements zcash/zips#977 (signed requests). Offered a Zafe prototype +
      draft ZIP if there's interest; nothing built until they answer
- [ ] frost#1082 (resharing proposal posted 2026-10-03): answered johnnynanjiang's
      revocation question 2026-10-04 (only a new key + moving the funds revokes old shares;
      https://github.com/ZcashFoundation/frost/issues/1082#issuecomment-5982780783). Follow
      up if ZF wants a spec
- [ ] frost-tools#433 (group description + threshold sent to DKG participants): design
      proposed 2026-10-03, offered to implement; waiting for answers to its two questions
      (`-d` mismatch: error or override; breaking older frost-client participants) before
      the PR
- [ ] Zakura's faster prover (V7): **supports Ironwood now** (zakura-core/common 2.2.0,
      2026-09-30, forked from orchard 0.15.5; MIT/Apache-2.0). Optimized Halo 2/Pasta with
      AArch64 assembly; zakura-bench (i7, 1 core): warm 2-action Ironwood proof 2.85 s →
      0.57 s, key build ~2.2 s → ~0.3 s. Adopting it means swapping the whole stack to the
      `zakura-*` forks (orchard, pczt, client-backend/sqlite still rc, keys, reddsa with
      `frost`), as Vizor did; mixing with crates.io types duplicates them. Next: spike
      branch, `scripts/android-bench.sh`, regtest e2e + zcash-sign cross-check. No Android
      numbers published yet.

## Known issues and tech debt

- [x] **`sk` agreement: the last contributor can bias `sk`** (fixed 2026-10-01: commit in
      round 1, compare in the echo, check on reveal; `DKG_ROUND1` 3) (found 2026-10-01; same
      issue str4d raised on zips#895). `keygen::combine_vault_secret` hashes the `r_i`,
      which are sent after the DKG without a prior commitment, so a member that waits
      can grind its own. Low impact (it learns `sk` anyway, can't spend alone), but fix
      before mainnet: commit to `H(r_i)` in DKG round 1 (bump `DKG_ROUND1`), reveal,
      abort on mismatch. `docs/stack-design.md` §7, `docs/stack-plan.md` phase 0
- [x] Height-gated wallet sync (2026-09-30): the Home poll asks lightwalletd for the tip
      (bridge `chain_tip`, one `GetLatestBlock`) and skips `sync_vault` when it equals the
      last synced height; a changed proposal list (`ProposalsNotifier` fingerprint →
      `VaultNotifier.markDirty`), a failed sync, resume and explicit actions still sync in
      full. Not measured on a device yet

- [x] **First sync after keygen stuck on "Syncing..."** (2026-09-30): reproduced on the
      Nothing phone (release, testnet): the breadcrumb said `sync failed at wallet key`
      after the 6-minute guard, and the very next `walletKey` call answered at once. A
      `flutter_secure_storage` call got no reply (Java worker idle, no Dart lock), most
      likely its async `initialize` racing the background engine's own plugin instance on
      the same prefs/Keystore (that instance logged "decryption failed"). Fix:
      `ZafeSecureStore` wraps every call in a 10 s timeout plus one retry
      (`_PatientStorage`). Upstream report worth filing with a minimal repro *(you?)*
- [x] A payment to the vault's **own address** failed every member's check (verification
      counts the output as change): `node::propose` now refuses it up front
      (`WalletError::Payment` → `InvalidInput`, "that is this vault's own address"),
      tested in `bridge_e2e`. The Send screen now checks too (2026-10-07: the recipient step blocks Continue, `_propose` refuses a batch row)

- [x] `AppButton` label was a separate node in accessibility trees (2026-09-30): `Focus`
      sat outside the `MergeSemantics` and added its own unlabeled focusable node. Now
      `MergeSemantics(Semantics(button, enabled, onTap, Focus(...)))`, the detector
      excluded from semantics; `test/app_button_semantics_test.dart` checks one node.
      Upstream Vizor `4bff2e7` has no `Semantics` at all: keep ours when resyncing.
      Still worth a TalkBack pass on a device
- [ ] Proposal `created_at` is the proposer's clock (display only, untrusted)
- [ ] Wallet DB access is serialized with one global lock in the bridge; fine for one
      vault, revisit for multiple vaults
- [x] Sync error copy (done 2026-09-30): Home's status names the cause (offline, Zcash
      server / relay unreachable, TLS, timeout, server behind, wrong network, wallet
      storage, update needed) from typed bridge errors (`ZafeError.endpoint` +
      new kinds, classified from error types in `zafe_core::net`); tapping it opens a sheet
      with the reason, server, last sync, relay last reached, raw detail, "Try again" and
      "Server settings". Follow-up: a Tor-unavailable kind once Tor lands
- [x] Copied Vizor `lib/src/core` resynced to `chainapsis/vizor-wallet` @ `4bff2e7`
      (was `ff02152`), keeping Zafe's button semantics; `docs/vizor-reference.md` §11.
      Also adopted: transaction progress screen (`/proposal/:id/send`), activity screen
      with Vizor's sections (`/activity`), entry-card backup reminder, destructive error
      toasts, `DecimalAmountInputFormatter`, full-address verify sheet
- [x] Zafe visual identity (2026-09-30): slate + jade palette, Space Grotesk / DM Sans /
      JetBrains Mono, rounded-rect buttons, vault-dial home card, left-aligned page titles,
      Vizor references removed from code comments; illustrations redrawn in the palette
- [x] Identity follow-ups: proposal page and send review use the payment card (dark vault
      card, amount + compact recipient), approvals block with signer dots, and signer rows
      with key-derived tiles (also on the home Signers card); icon and splash done.
      Previews without a device: `flutter test tool/screens/proposal_render_test.dart`
- [x] Member tiles stay derived from keys (decided 2026-10-01): the tile is the key's
      fingerprint, the same on every device, while a name is a label anyone can set; names
      already show next to the tile
- [ ] iOS: `xyz.zafe/modal_corners` has no Swift handler yet (Dart falls back to fixed
      corners); port Vizor's `NativeModalCorners` when iOS work starts
- [x] CI: `.github/workflows/ci.yml` runs `cargo fmt --check`, clippy `-D warnings`,
      `cargo test --workspace`, the ZIP 2005 Python check, `flutter analyze` and
      `flutter test test` (regtest/Docker tests stay manual). No GitHub remote yet, so it
      runs once the repo is pushed
- [x] CI Rust job failing since the Tor commit (2026-10-01): `ld terminated with signal 7
      [Bus error]` linking the test binaries = runner disk full. Fixed with a "Free disk
      space" step and `CARGO_PROFILE_DEV_DEBUG=line-tables-only`; a "Disk usage" step
      prints what's left
- [x] Site deploys to Vercel from GitHub Actions (`.github/workflows/site.yml`, 2026-10-01)
  - [ ] **(you)** Create the Vercel project and token; set `VERCEL_TOKEN` (secret),
        `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID` (variables); add the domain
        (`infra/site/README.md` → Deploy → Vercel)
- [ ] Agent-device flows are manual; capture them as a repeatable script
      (`scripts/app-harness.sh` covers the backend side). Model: Vizor's app-level regtest
      E2E, `integration_test/payment_uri_prefill_test.dart` driven by
      `scripts/e2e/flutter-ios-regtest-mobile-*.sh`
- [x] Background check ANR (2026-09-30, emulator under heavy host load): Android reported
      "No response to onStartJob" for the WorkManager vault check. Likely load, but check
      that the job's startup (RustLib.init, Firebase, secure storage reads) doesn't block
      the main thread before WorkManager gets its answer
      2026-09-30 analysis: "No response to onStartJob" is Android's main thread waiting
      before any Dart runs; starting the background Flutter engine in a **debug** build
      (JIT) under heavy host load is the likely cause. Closed 2026-10-01: the release
      build on the phone (testnet dry run) showed no ANR; and the main-thread part is the
      plugin's (workmanager_android 0.10.9 `BackgroundWorker.startWork`: Flutter loader +
      engine + plugin registration), while our check runs afterwards on the engine's
      Dart thread. Reopen only if it shows up in a release build
- [x] Relay: storage quotas for the hosted tier (2026-09-30): `zafe_relay::quota`, per
      mailbox: undelivered envelopes per recipient (indexed count), delivery bytes and
      log bytes (running counters on `mailboxes`, schema `RELAY_DB` 2 with a migration
      from 1; pruning gives bytes back). 507 → `RelayClientError::StorageFull` →
      `ZafeErrorKind::RelayStorageFull` with copy; env overrides in the binary. Retention
      stays 30 days
- [x] **Versioned formats** (2026-09-30): every wire and stored format carries a version
      tag (`zafe_proto::version`; inventory and bump rules in AGENTS.md), signed where a
      downgrade matters (envelope and log-entry headers, relay requests, descriptor).
      Unknown versions fail with a typed `UnsupportedVersion`; the relay answers 426 and
      the app shows "update the app" (`ZafeErrorKind::UpdateRequired` / `RelayOutdated`);
      replay ignores newer events after `Created`. Unversioned pre-release data is not
      readable: reset devices, harness and relay DBs
- [x] "Update Zafe" notice on Home when the log has entries from a newer version
      (`list_proposals` now returns `ProposalList { items, newer_version_entries }`;
      `ProposalsState.newerVersionEntries`; Home `_NoticeCard`, shared with the backup
      reminder). No store link yet
- [ ] Versioning follow-ups: decide a migration
      policy (which old versions each decoder keeps) before external testers; a
      `/v1/version` endpoint or response header so the app can warn before the first
      failing call *(idea)*

- [x] **No telemetry + local diagnostic report** (2026-10-07): CI guard
      `scripts/check-no-telemetry.sh`; scrubbed on-device crash log with Settings > "Diagnostic report"
      (view, share, copy, clear). Done 2026-10-07: background-engine errors (own file, merged on read),
      Rust panic notes (`app/rust/src/diag.rs`), "No telemetry" in README + site FAQ/Security,
      render preview (`tool/screens/diagnostics_render_test.dart`). Open: the Rust hook and the
      bridge wiring are unverified (no cargo run); check a panic and a background error on a device.

## Ideas (not decided)

- *(idea, 2026-10-01)* **N-of-M ring emblem**: a ring of M segments with T filled as a
  per-vault mark (logo study, `docs/brand.md` §6). Not adopted: the per-vault picture
  emblems already took the avatar, and the balance card shows the threshold as dots.
- *(idea, 2026-10-01)* **Quorum Z splash animation**: the three strokes of the Quorum Z
  arriving one by one, resolving into the Seam icon.

- *(idea, proposed 2026-09-29)* **One-tap approvals: sign at approval time.** Members
  pre-publish pools of FROST nonce commitments (FROST's preprocessing round, done ahead
  of time). When a proposal is built, its signing packages are fixed, so an approver's tap
  produces its signature shares immediately; shares go into the encrypted log. Either the
  proposer fixes the signer set, or each approver signs one package per t-subset that
  includes them (C(n-1, t-1) shares: 2-of-3 → 2, 3-of-5 → 6, 4-of-7 → 20), so any t
  approvers complete it with no second round. Each nonce is used once, with consumption
  ordered by the log. Fall back to today's interactive round for large vaults. Pairs with
  auto-submit: the approval that completes the threshold aggregates, proves and
  broadcasts; a vault setting keeps "send manually" for timing control.

- *(idea)* Relay as a Tor onion service (pairs with the Tor slice; Vizor routes broadcasts on
  an isolated circuit, `open_isolated_lwd_channel` in `rust/src/wallet/sync_engine/lwd.rs`)
- *(idea)* Fiat values next to amounts (Vizor shows USD; needs a price source, Tor-aware;
  Vizor fetches through `lib/src/core/network/network_http_client.dart`)
- [x] Sensitive-screen protection, Android (2026-09-30): `SecureScreen` (counted) sets
  `FLAG_SECURE` through `xyz.zafe/secure_screen` (`MainActivity`) on /setup, /join,
  /export and /restore; checked on the emulator (flag on while open, off after).
  Still open: iOS capture shield and screenshot warning, an app-switcher privacy overlay.
  Earlier idea text: Sensitive-screen protection (Vizor's `SensitivePrivacyOverlay` + Android
  `FLAG_SECURE`) for invite, safety number and backup screens. Upstream now also blocks
  capture on iOS (`SecureScreenshotShield` in `ios/Runner/AppDelegate.swift`) and warns
  after screenshots (`lib/src/core/platform/screenshot_observer.dart`)
- *(idea)* "Keep screen awake" while collecting signatures (Vizor has this setting):
  `lib/src/providers/sync_keep_awake_provider.dart`, `lib/src/services/native_screen_awake.dart`
  (ETA-based prompt, privacy lock after 1 min idle)
- *(idea)* Show which signers are online / last seen (relay metadata; privacy trade-off)
- *(idea)* Explorer link for txids with a custom explorer setting
  (`lib/src/features/settings/screens/mobile/mobile_explorer_screen.dart`,
  `lib/src/core/config/zcash_explorer.dart`; default CipherScan, `{txid}` templates) and
  txids in explorer byte order
- *(idea)* Proposal comments/discussion thread in the encrypted log
- *(idea)* Scheduled / recurring payments (grant programs pay in milestones). Model: Vizor's
  background outbox of fully signed txs with `scheduledHeight` / `expiryHeight` and a
  `needsResign` state (`ios/Runner/BackgroundMigrationOutbox*.swift`,
  `BackgroundMigrationManager.swift`)
- *(idea)* Grantee payment requests: "Request ZEC" on the vault receive screen with a ZIP-321
  builder that never emits `label`/`message` (`lib/src/core/zcash/zip321_payment_request_builder.dart`,
  `lib/src/features/receive/widgets/mobile/receive_request_sheet.dart`)
- [x] Recipient QR and payment links (2026-09-30): "Scan QR code" on Send's recipient step
  and a "Choose image" option on every scanner (a shared QR picture or screenshot,
  `MobileScannerController.analyzeImage`); pasting a `zcash:` link works too. Bridge
  `parse_payment_request` (ZIP 321 via `zip321`: amount, memo, several recipients → a
  batch; every address checked for the vault network), tested in
  `app/rust/tests/payment_request.rs`. The invite card has "Share QR image" (PNG, white
  background, quiet zone) now that the screen blocks screenshots; Join's scanner can read
  it with "Choose image". Emulator: picked a QR image → amount, recipient and memo filled.
  Not done: live-camera test on a phone. Opening `zcash:` links from other apps: done
  2026-10-04 (see below)
- [x] `zcash:` links from other apps + app lock (2026-10-04): intent filter / URL scheme,
  payment request screen (warning, unverified label/message, vault picker, explicit
  "I know who sent this" tick, then Send prefilled), link held until unlock and dropped
  after 10 min; app-wide lock at launch and after a background delay (Settings → Lock
  app). AGENTS.md "App lock" / "Payment links". Done the same day: content hidden in
  the recent-apps view, and a cheap funds check per vault on the request page.
  Follow-ups: iOS untested (never built); exact fee via a build-without-logging
  precheck (idea below); (done 2026-10-07: a 404 "unknown mailbox" is
  `ZafeErrorKind::VaultNotOnRelay`, "This vault isn't on this relay"); on the emulator the app
  sometimes shows a ~12% white veil (#262929 instead of #080B0B) over the Flutter
  surface, also with the main build and not in the app's view tree (cause unknown)
- *(idea)* **"Pay with Zafe" verified link** `https://zafe.cash/pay#<zcash: URI>` (App Link;
  zafe.cash already serves `assetlinks.json` for `/join`): only the genuine Zafe opens it,
  unlike `zcash:`. Add the `/pay` path to the manifest, route it into the payment request
  screen, a static no-analytics fallback page, a dapp snippet (incl. the native-app path:
  explicit intent + `PackageManager.hasSigningCertificate`), AASA once there's an Apple
  team ID. Build when the first dapp/merchant asks. Same thread: a check code + number
  matching only if a ZIP gains traction (zodl-project#60)
- *(idea)* Propose from a request: a scanned/pasted/opened `zcash:` URI shows a card that
  prechecks and builds the proposal up front (`lib/src/features/send/widgets/payment_request_host.dart`,
  `lib/src/features/send/services/payment_request_precheck.dart`)
- *(idea)* One link intake for `zafe://` invites and `zcash:` URIs: classify host before
  scheme, exact route allowlist, pure drain policy for locked/onboarding states
  (`lib/src/core/navigation/incoming_link_dispatch.dart`, `payment_uri_drain_policy.dart`);
  pairs with M1 "Invite by link"
- *(idea)* Put invite secrets in the URL fragment as a compact positional payload so a link
  host never sees them (`docs/compact-gift-links.md`)
- *(idea)* Auditor export of the vault UFVK behind the passcode, reusing Vizor's copy
  (`lib/src/features/settings/viewing_key_copy.dart`, `mobile_viewing_key_screen.dart`);
  complements M3 payment disclosures
- *(idea)* Batch payments UI from Gift Card groups: "Per card × N + Network fee = Total",
  fee estimated for the whole output set, CSV export
  (`lib/src/features/payment_links/widgets/payment_link_bulk_desktop_flow.dart`,
  `services/payment_link_batch_export.dart`); feeds M2 batch payments
- *(idea)* Unknown broadcast results: "Payment status pending" / "Check status" (scan only),
  never offer a second send (`docs/gift-card-groups.md`,
  `lib/src/features/pay/screens/mobile/mobile_pay_submitted_screen.dart`)
- *(idea)* Durable checkpoint of the aggregated tx before broadcast, with a startup recovery
  loop (`signed_pending_broadcast` / `result_pending_ack`) and a claim registry
  (`rust/src/wallet/ledger/operations.rs`, `lib/src/features/ledger/services/ledger_operation_recovery.dart`)
- *(idea)* Honest signing stages from protocol boundaries, no timers/percentages
  (`docs/ledger/signing-phase-guidance.md`, `lib/src/features/ledger/services/ledger_signing_progress.dart`);
  voting's named submission stages (`lib/src/features/voting/screens/mobile/mobile_voting_submission_progress_screen.dart`)
- *(idea)* Proposal list UX from ballots: jump to question, unanswered-first, auto-advance
  countdown, "Resume to complete the submission" (`lib/src/features/voting/widgets/voting_proposal_navigation.dart`,
  `voting_auto_advance_indicator.dart`, `voting_resume_plan.dart`)
- *(idea)* Local notifications as a bridge before push, deduplicated per scope and kind
  (`ios/Runner/MigrationPreparationNotificationCoordinator.swift`)
- *(idea)* Evidence-based payee states ("Confirming", "Unverified" with a reason, used after
  6 confirmations; never inferred from empty history) (`docs/gift-card-claim-outcomes.md`,
  `docs/gift-card-usage-tracking.md`)
- *(idea)* Contact names on every address surface and "New address detected. Add to contacts"
  without guessing between duplicates; basis for member names and the M2 address book
  (`lib/src/features/address_book/models/address_book_label_lookup.dart`,
  `widgets/contact_name_inline.dart`, `lib/src/features/pay/models/pay_recent_recipients.dart`)
- *(idea)* Errors across the bridge as stable codes with a "retry is pointless" flag
  (`lib/src/features/ledger/ledger_error_codes.dart`, `ledger_failure_guidance.dart`), or
  typed like voting (`lib/src/services/voting/voting_rust_exception.dart`)
- *(idea)* Send-status screen with outcome haptics: `MobileTransactionProgressScreen`
  (`lib/src/core/widgets/mobile/mobile_transaction_progress_screen.dart`) and
  `AppHaptics.sendSuccess/sendFailure` (`lib/src/core/feedback/app_haptics.dart`)
- *(idea)* Customise-account step (name + avatar, generated default name) after vault
  creation/join (`lib/src/features/onboarding/mobile/mobile_customise_account_screen.dart`,
  `onboarding/create/account_persona_generator.dart`)
- *(idea)* Small mobile fixes: Android back-exit guard (`lib/src/core/navigation/mobile_exit_back_guard.dart`)
  and iOS number-pad Done bar (`lib/src/core/widgets/mobile/mobile_numeric_keyboard_toolbar.dart`)
- *(idea)* Private transaction-detail lookups via PIR once available for Ironwood
  (`lib/src/providers/enhance_pir_provider.dart`, `rust/src/wallet/sync_engine/enhancement/`)
- *(idea)* Design review tooling: Widgetbook galleries + headless PNG renders
  (`lib/widgetbook/`, `lib/figma_compare.dart`, `scripts/figma-compare.sh`)
- *(idea)* iOS Keychain `first_unlock_this_device` for shares from day one (Vizor migrated to
  it: `lib/src/core/storage/app_secure_store.dart`, `ios/Runner/KeychainAccessibilityMigrator.swift`)

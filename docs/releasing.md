# Releasing the Android app

Testnet APKs (tags `v*`) are built and signed by `.github/workflows/release.yml`.
**Mainnet beta APKs** (tags `mainnet-v*`, package `xyz.zafe.zafe`, relay
`https://relay.zafe.cash`) come from `.github/workflows/release-mainnet.yml`, with the same
signing key and `android-release` environment (its tag policy lists both `v*` and
`mainnet-v*`). It refuses to build unless `relay.zafe.cash/health` answers `ok`. The
release notes say "beta, not audited"; the external audit is still open
(`docs/tracker.md`, M2). A sideloaded APK carries our upload-key signature and a Play
install is re-signed by Play, so Play can't update a sideloaded install (uninstall first).

```bash
git tag v<version> && git push origin v<version>                  # testnet build first
git tag mainnet-v<version> && git push origin mainnet-v<version>  # same commit, after the dry run
```

**Testnet first.** Testnet is the staging step for mainnet: tag the commit `v<version>`,
install the testnet APK, run the dry run (create, receive, propose, sign, send, restore),
and only then tag `mainnet-v<version>` on the **same commit**. For the restore step, save the
backup file, or wipe and paste within 60 s of **Copy as text**: Zafe clears a copied
secret after 60 s (`kSecretClipboardLifetime`), and in a 2-of-2 a lost seat can't be
repaired. A CLI member can be the co-signer (`zafe vault join` takes the raw
`zafe-invite-v…` text, not the link). `release-mainnet.yml`
refuses a commit that has no `v*` tag. The testnet app installs next to the mainnet one
as "Zafe Testnet" (`xyz.zafe.zafe.testnet`) and shows amounts in TAZ. Its release notes
tell testers they need testnet coins and link [Fauzec](https://fauzec.com/) (1 TAZ per
address every 24 h, a Turnstile check; paid an Ironwood vault on 2026-10-27). The Valar
faucet has paid nobody since 2026-10-23
([valar-testnet-faucet#5](https://github.com/valargroup/valar-testnet-faucet/issues/5)). Each
open proposal locks a whole note, so a test with two proposals at once needs two payouts.

**Invite page per host.** `testnet.zafe.cash/join` serves `join-testnet.html` (a TESTNET
badge, "test coins with no value", the testnet download) and `zafe.cash/join` the mainnet
page: one deployment, the Worker picks the page by host (`infra/site/worker/index.js`,
`TESTNET_HOST` in `wrangler.jsonc`).

**Site links.** `/download` links Google Play, the newest `mainnet-v*` APK, the newest
`v*` testnet build and the iPhone waitlist. `site.yml` looks the releases up with
`gh release list` at build time, and both release workflows start `site.yml` after
publishing (`actions: write`).

## Google Play

App `xyz.zafe.zafe` in the organization account "Zafe". The first
bundle (0.1.16, version code 2018) was uploaded by hand to internal testing, which also
enrolled Play App Signing (the API can't create a new app's first release). Since then
every `mainnet-v*` tag also builds an App Bundle (version code 3000 + run number) and
`release-mainnet.yml` uploads it to the **internal** track with a Play publisher service
account (Play Developer API on; key only in the `android-release` secret
`PLAY_SERVICE_ACCOUNT_JSON`). In Play Console that account may only release to testing tracks:
**production is always a manual promotion** of a tested internal build. Release builds
ship arm64 native libraries only (`build.gradle.kts`), so Play offers the app to arm64
phones only. Play re-signs installs with its own key:
`48:8C:C7:3B:92:D0:A4:7F:96:89:F9:4C:DE:96:CA:E4:38:8E:C1:B8:90:E3:DB:07:92:49:5C:AE:58:32:8B:BC`
(App integrity → App signing). The site's `assetlinks.json` is built from the repo variable
`ZAFE_ANDROID_CERT_SHA256` (comma-separated: upload key, then Play's key), and a Play install
reports `zafe.cash: verified` (checked 2026-10-16, `adb shell pm get-app-links xyz.zafe.zafe`).
First production release 0.1.16 (2018) sent for review 2026-10-16 with managed publishing
on, all countries. Play Console answers (listing, Data safety, financial features: crypto
wallet + "non-custodial software wallet" confirmed per country, no licences; advertising ID:
No) are recorded in the private GTM repo, `content/play-listing/listing.md`. A new app's first
release can't use a staged rollout; later releases can.

## Release class: optional or essential

Every mainnet version has `docs/releases/<version>.md`: `class: optional` (Play in-app
update priority 2) or `class: essential` (priority 5), then the notes. Format and when to
use each class: [`releases/README.md`](releases/README.md). `release-mainnet.yml` refuses
a tag without it, uploads to internal with that priority and the notes as "What's new",
and puts the notes at the top of the GitHub release.

Production: run **Promote to Google Play production** (`play-promote.yml`, inputs: version,
rollout percent; essential must be 100). It waits for approval in the `play-production`
environment, then `scripts/play_promote.py` assigns the internal release named `<version>` to
production with the class's priority and notes. Its key belongs to a second service account,
the only one with production rights in Play Console; the uploader can reach testing tracks
only. The Console can't set the priority, so promote with the workflow, not by hand. With
managed publishing on, an approved release still waits for **Publish** in the Console.

## Locally

`app/android/key.properties` (gitignored) with `storeFile`, `storePassword`, `keyAlias`,
`keyPassword` makes `flutter build apk --release` sign with that key; without it release
builds use the debug key (fine for testing, but such an APK can't update a real one).
Always pass `--build-number=1` locally: a signed local build with a code above CI's
(4100 + run number) makes every later release a downgrade on that phone, and the only way
out is uninstalling, which deletes its vaults (happened 2026-10-16 and 2026-10-27).

```bash
cd app && flutter build apk --release --split-per-abi --target-platform android-arm64 \
  --dart-define=ZAFE_NETWORK=test --dart-define=ZAFE_RELAY_URL=https://<relay> \
  --dart-define=ZAFE_LINK_HOST=<site host> --build-number=1   # link host: required for releases
```

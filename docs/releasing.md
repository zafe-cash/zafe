# Releasing the Android app

Testnet APKs (tags `v*`) are built and signed by `.github/workflows/release.yml`.
**Mainnet beta APKs** (tags `mainnet-v*`, package `xyz.zafe.zafe`, relay
`https://relay.zafe.cash`) come from `.github/workflows/release-mainnet.yml`, same signing
key and `android-release` environment (its tag policy lists both `v*` and `mainnet-v*`);
it refuses to build unless `relay.zafe.cash/health` answers `ok`. The release notes say
"beta, not audited". A sideloaded APK carries our upload-key signature; a Play install is
re-signed by Play, so a sideloaded install can't be updated from Play (uninstall first).
The external audit is still open (`docs/tracker.md`, M2).

```bash
git tag mainnet-v0.1.0 && git push origin mainnet-v0.1.0
```

**Testnet first.** Testnet is the staging step for mainnet: tag the commit `v<version>`
(testnet APK on `testnet.zafe.cash` links), install it, run the dry run (create, receive,
propose, sign, send, restore), and only then tag `mainnet-v<version>` on the **same commit**.
`release-mainnet.yml` refuses a commit that has no `v*` tag. The testnet app shows
"VAULT BALANCE · TESTNET" on its vault card so it can't be mistaken for the mainnet one, and its
release notes tell testers they need testnet coins (TAZ) and link the Zcash testnet guide
(the Valar faucet, https://faucet.testnet.valargroup.dev/: 0.125 TAZ per request, one per IP per 24 h; its payout reached an Ironwood vault on 2026-09-30; each open proposal locks a whole note, so a test with two proposals at once needs two payouts).

**Invite page per host.** `testnet.zafe.cash/join` serves `join-testnet.html` (a TESTNET badge,
"test coins with no value", the testnet download) and `zafe.cash/join` the mainnet page: one
deployment, one Vercel route with a host match (`vercel-output.sh`, `ZAFE_TESTNET_HOST`).

**Site links.** "Get Zafe" and "Try the testnet build" point at the newest `mainnet-v*` and
`v*` release; `site.yml` reads them with `gh release list` at build time, and both release
workflows start `site.yml` after publishing (`actions: write`). Store badges say "Soon"
until the listings exist; launch news goes out on X (@zafecash, `src/config.ts`), no
waitlist or address collected.

## Google Play

App `xyz.zafe.zafe` in the organization account "Zafe". The first
bundle (0.1.16, version code 2018) was uploaded by hand to internal testing, which also
enrolled Play App Signing (the API can't create a new app's first release). Since then
every `mainnet-v*` tag also builds an App Bundle (version code 3000 + run number) and
`release-mainnet.yml` uploads it to the **internal** track with the service account
a Play publisher service account (Play Developer API on; key only in the `android-release` secret
`PLAY_SERVICE_ACCOUNT_JSON`). In Play Console it may only release to testing tracks:
**production is always a manual promotion** of a tested internal build. Release builds
ship arm64 native libraries only (`build.gradle.kts`), so Play offers the app to arm64
phones only. Play re-signs installs with its own key: `48:8C:C7:3B:92:D0:A4:7F:96:89:F9:4C:DE:96:CA:E4:38:8E:C1:B8:90:E3:DB:07:92:49:5C:AE:58:32:8B:BC`
(App integrity → App signing). The site's `assetlinks.json` is built from the repo variable
`ZAFE_ANDROID_CERT_SHA256` (comma-separated: upload key, then Play's key), and a Play install
reports `zafe.cash: verified` (checked 2026-10-16, `adb shell pm get-app-links xyz.zafe.zafe`).
First production release 0.1.16 (2018) sent for review 2026-10-16 with managed publishing
on, all countries. Play Console answers (listing, Data safety, financial features: crypto
wallet + "non-custodial software wallet" confirmed per country, no licences; advertising ID:
No) are recorded in the private GTM repo, `content/play-listing/listing.md`. A new app's first
release can't use a staged rollout; later releases can.

## Locally

`app/android/key.properties` (gitignored) with `storeFile`, `storePassword`, `keyAlias`,
`keyPassword` makes `flutter build apk --release` sign with that key; without it release
builds use the debug key (fine for testing, but such an APK can't update a real one).

```bash
cd app && flutter build apk --release --split-per-abi --target-platform android-arm64 \
  --dart-define=ZAFE_NETWORK=test --dart-define=ZAFE_RELAY_URL=https://<relay> \
  --dart-define=ZAFE_LINK_HOST=<site host>   # required for releases
```

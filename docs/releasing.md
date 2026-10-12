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
(no verified public faucet, 2026-10-12).

**Site links.** "Get Zafe" and "Try the testnet build" point at the newest `mainnet-v*` and
`v*` release; `site.yml` reads them with `gh release list` at build time, and both release
workflows start `site.yml` after publishing (`actions: write`). Store badges say "Soon"
until the listings exist; launch news goes out on X (@zafecash, `src/config.ts`), no
waitlist or address collected.

## Locally

`app/android/key.properties` (gitignored) with `storeFile`, `storePassword`, `keyAlias`,
`keyPassword` makes `flutter build apk --release` sign with that key; without it release
builds use the debug key (fine for testing, but such an APK can't update a real one).

```bash
cd app && flutter build apk --release --split-per-abi --target-platform android-arm64 \
  --dart-define=ZAFE_NETWORK=test --dart-define=ZAFE_RELAY_URL=https://<relay> \
  --dart-define=ZAFE_LINK_HOST=<site host>   # optional
```

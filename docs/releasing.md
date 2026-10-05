# Releasing the Android app

Releases are testnet APKs on GitHub Releases, built and signed by
`.github/workflows/release.yml`. Mainnet builds wait for the external audit, a hosted relay and a small mainnet dry run
(`docs/tracker.md`, M2).

## One-time setup (you)

1. **Deploy the relay** (`infra/relay/README.md`) and note its `https://` URL.
2. **The upload key exists** (made 2026-10-01: alias `zafe`, cert SHA-256
   `D8:A2:F2:C1:…:F6:9C`, on the maintainer's machine as `~/.config/zafe/zafe-upload.jks`
   + `zafe-upload.password`; PKCS12, key password = store password). **Don't make a new
   one**: zafe.cash's `assetlinks.json` lists this fingerprint (repository variable
   `ZAFE_ANDROID_CERT_SHA256`), and every future release must be signed with the same
   key, or phones refuse the update. Keep the `.jks` and its password in a password
   manager, not in the repo. How it was made, for reference:

   ```bash
   keytool -genkeypair -keystore zafe-upload.jks -alias zafe \
     -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Zafe"
   base64 -w0 zafe-upload.jks > zafe-upload.jks.b64
   ```
3. **GitHub settings** of the repo (Settings > Secrets and variables > Actions):
   - Variables: `ZAFE_RELAY_URL` = `https://testnet.relay.zafe.cash` (set 2026-10-06). `ZAFE_LINK_HOST` =
     `zafe.cash` (set 2026-10-01; the site serves `assetlinks.json` with this key's
     fingerprint). Without it, invites are `zafe://` links, which only work where Zafe
     is installed.
   - Secrets (not set yet; set them before the first release), from `~/zafe`:

     ```bash
     d=~/.config/zafe
     base64 -w0 $d/zafe-upload.jks | gh secret set ANDROID_KEYSTORE_BASE64
     gh secret set ANDROID_KEYSTORE_PASSWORD < $d/zafe-upload.password
     gh secret set ANDROID_KEY_PASSWORD < $d/zafe-upload.password
     printf zafe | gh secret set ANDROID_KEY_ALIAS
     ```
   - Optional secret `GOOGLE_SERVICES_JSON`: the contents of
     `app/android/app/google-services.json` (Firebase project `zafe-18c4d`), for push.
     The relay then needs the FCM service account too.

## Each release

```bash
# bump `version:` in app/pubspec.yaml if you like; the tag decides the version name
git tag v0.1.0
git push origin main v0.1.0
```

The workflow builds `zafe-<version>-testnet-arm64.apk`, publishes it as a **pre-release**
with a `.sha256` file and the signing certificate fingerprint in the notes. The build
number is the workflow run number, so each release installs over the previous one. You
can also start it from the Actions tab with a version (it creates the tag).

## Locally

`app/android/key.properties` (gitignored) with `storeFile`, `storePassword`, `keyAlias`,
`keyPassword` makes `flutter build apk --release` sign with that key; without it release
builds use the debug key (fine for testing, but such an APK can't update a real one).

```bash
cd app && flutter build apk --release --split-per-abi --target-platform android-arm64 \
  --dart-define=ZAFE_NETWORK=test --dart-define=ZAFE_RELAY_URL=https://<relay> \
  --dart-define=ZAFE_LINK_HOST=<site host>   # optional
```

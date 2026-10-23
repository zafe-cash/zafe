# App, art and site notes

Detailed notes for the Flutter app, Android, illustrations and the website, moved out of
`AGENTS.md` (which keeps only the rules). Read the part you need before working there; add
new findings here, and a one-line rule to `AGENTS.md` only if breaking it causes a bug.

## Android platform

- **Android build on Flutter 3.47 (2026-10-22)**: AGP 9.1.1, Gradle 9.3.1, Kotlin Gradle plugin 2.4.0, set up
  as Flutter 3.47's own template (`android.newDsl=false`, `android.builtInKotlin=false`: Flutter applies
  `kotlin-android`; the app's Gradle file has `kotlin { compilerOptions }`, no `kotlinOptions`). Built-in
  Kotlin waits for **firebase_core** and **workmanager_android**, which still apply KGP (Flutter warns that a
  future version will refuse them). Optimized resource shrinking is AGP 9's default. Gradle 9 removed
  `Project.exec()`: our cargokit copy (`rust_builder/cargokit/gradle/plugin.gradle`) runs through an injected
  `ExecOperations` (upstream cargokit has no fix). The Rust plugin module (`rust_builder/android/build.gradle`)
  compiles against SDK 36 / min 24 / Java 17: AGP 9 refuses androidx dependencies that need SDK 34+.
  **`shared_preferences_android` is pinned to 2.4.23**: 2.5.x calls its own Kotlin over JNI with no R8 keep
  rules, so a minified release fails every prefs call (`NoSuchMethodError ... SharedPreferencesApiRegistrar`)
  and the app hangs at start; release builds catch this, `flutter test` doesn't. `permission_handler` stays on
  12 (13 needs compileSdk 37; we only call `openAppSettings()`).
- **FCM targets are installation IDs (2026-10-22)**: FCM deprecated registration tokens (v1 `token`: "Use fid
  instead"). The manifest sets `firebase_messaging_installation_id_enabled` (in this mode `getToken()` throws);
  the app listens to `onRegistered` and calls `register()` at start, on a relay change and after a new vault
  (it re-fires with the current FID); it registers `platform: "fcm-fid"` (`PushPlatform::FcmFid`, appended);
  the relay sends it in v1's `fid` field. One target per member: an FID replaces the old token. **A relay
  without `FcmFid` rejects it, so promote the relay before an app that registers FIDs ships.**
- **System bars (2026-10-22)**: `MainActivity` calls `enableEdgeToEdge` (`SystemBarStyle.auto`: both bars
  transparent on Android 10+, the window colour below). **`MaterialApp` calls `SystemChrome.setSystemUIOverlayStyle(
  SystemUiOverlayStyle.dark/.light)` on every build** (flutter `material/app.dart:1012`), and both carry
  `systemNavigationBarColor: 0xFF000000` (`services/system_chrome.dart:316-330`); the engine applies it below
  API 35 (`PlatformPlugin.java:571-578`). That was the black band behind the gesture bar on Android 14 (any
  Flutter app, 0.1.21 too); a one-off `SystemChrome` call from above `MaterialApp` loses to it. So the app's
  style is an `AnnotatedRegion<SystemUiOverlayStyle>` (`app_theme_host.dart`), which `RenderView` sends after
  each frame's build, with a transparent nav colour; test: `android_system_bars_test.dart`. Android 15+ ignores
  bar colours.
- **Testnet is a separate Android app**: `ZAFE_NETWORK=test` builds get
  `applicationId xyz.zafe.zafe.testnet` and the label "Zafe Testnet" (`build.gradle.kts`
  reads the dart-define; the manifest label is `${zafeAppLabel}`), so it installs next to
  the mainnet app and can't update it. Mainnet and regtest/dev builds stay
  `xyz.zafe.zafe` (the harness and `adb` commands use that). The site's
  `assetlinks.json` lists both packages. `google-services.json` must contain a client
  for whichever package you build, or the Google Services Gradle plugin fails.
- **Zcash server list** (2026-10-06, Vizor-style): `core/config/lightwalletd_presets.dart`
  holds the public servers per network (first = the build default; mainnet: Zec Rocks
  global/na/eu/ap/sa, Stardust us/eu, Zcash Explorer; testnet: only testnet.zec.rocks
  answered). Settings > Zcash server lists them with a latency probe each
  (`probeLightwalletd`, a `checkLightwalletd` call, through Tor when it's on) plus
  "Custom server". Sync fails over (`serverFailoverProvider`, from `VaultNotifier.sync`)
  to the next listed server on unreachable/timeout/TLS/server-behind, at most once per
  5 min, never away from a custom URL, and the app toasts the move. Background checks
  don't fail over. Re-probe the list (`grpcurl ... GetLightdInfo`) before adding hosts:
  Stardust eu2/jp and two community servers were dead or had broken TLS.
- **Recording the phone for review**: run `adb shell screenrecord --time-limit 180 /sdcard/x.mp4` as a background
  task (Bash `run_in_background`); started with `&` inside a foreground command it is killed when that returns and
  leaves an empty file. Stop early with `adb shell pkill -INT screenrecord`. Capture-protected screens (setup, keys,
  secrets) record black. The recording shows whatever is on screen, other apps included: look only at Zafe.

## Tor (full notes)

- **Tor ("Use Tor", Settings > Privacy; default off, per device)**: arti embedded through
  `zcash_client_backend`'s `tor` feature (arti-client 0.35, no tor binary; +252 crates,
  ring only, zstd/xz C code builds with the NDK; no pinned crate moved). Size: arm64
  `librust_lib_zafe.so` stripped 24.7 → 34.7 MB (+10 MB; gzip 11.8 → 15.8 MB, i.e.
  about +4 MB to download). Policy in
  `zafe_core::tor`, process-wide, **fail-closed**: `tor::request()` flips the route
  before any bootstrap (and cancels a `CancellationToken` every direct connection holds);
  from then on `tor::route()` returns the Tor client or `Blocked::{Connecting, Failed}`
  (→ `NetFailure::TorConnecting/TorFailed` → `ZafeErrorKind::TorConnecting/TorFailed` →
  `SyncFailureKind::torConnecting/torFailed`), never direct. A request waits up to
  `ROUTE_WAIT` (20 s) for a bootstrap in progress; a failed one answers at once.
  `tor::enable(dir, budget)` is bounded (arti retries forever on a blocked network),
  idempotent (instant when Ready), serialized, abandons when Tor is turned off
  mid-bootstrap, and on failure leaves status `Failed` with the route still Tor. Choke
  points: `wallet::connect` (every lightwalletd use: sync, send, mempool watch,
  `check_server`, `chain_tip`, endpoint checks) and `RelayClient::exchange` (every relay
  request incl. `/v1/wait`, push registration, `/health`). Direct paths: lightwalletd
  uses tonic `connect_with_connector(tor::DirectConnector)` (our TCP, tonic adds TLS;
  I/O wrapped in `DirectIo`, which errors once the token is cancelled, and the
  connector refuses tonic's reconnects); the relay keeps **reqwest** for direct and wraps
  each request in `DirectLease::guard` (dropping the future drops the connection; pooled
  idle connections are never used while Tor is on). Through Tor: lightwalletd via
  `Client::connect_to_lightwalletd` (90 s cap), relay via `Client::http_get/http_post`
  (why not a pooled hyper client over arti: `zcash_client_backend::tor::Client` doesn't
  expose its arti client, and a second arti instance would duplicate it; cost: a new Tor
  stream + TLS handshake per relay request, ~1 s; timeouts get +30 s). Relay and
  lightwalletd use separate isolated circuits. `RelayClient::with_extra_root` is not
  honoured over Tor (bundled roots only). arti refuses local addresses, so regtest
  (127.0.0.1 / 10.0.2.2) can't work with Tor on: it fails closed, as it should. App:
  pref `zafe_use_tor`; `main()` calls `torRequest()` right after `RustLib.init()`
  (before anything can connect), `torLifecycleProvider` (kept alive by `ZafeApp`)
  bootstraps without blocking the UI (180 s) and sets dormant on hide/pause (a request
  wakes arti by itself), retrying a failed Tor on resume. Background checks
  (`vault_watch.dart`) read the pref after `prefs.reload()`, then `ensureTorForBackground`
  (request, then enable within 60 s) and skip the run if Tor isn't connected. State dir:
  `<appSupport>/tor` (`ZafePaths.torDir`; Android backup/transfer already excluded; on
  mobile fs-mistrust is relaxed with `dangerously_trust_everyone`, the sandbox being the
  boundary). FCM push stays direct (Google, content-free). Tests: `tor::tests` (pure
  route decision, `DirectIo` cut, guard), `tests/tor_policy.rs` (own process: nothing
  reaches a server once Tor is requested, waiters wake on failure, a direct long poll
  and a lightwalletd call are cut; ignored live test: cold bootstrap ~14 s, testnet tip
  through Tor ~3.5 s, an HTTPS GET ~1 s), Dart `test/tor_setting_test.dart`; sheet
  preview `flutter test tool/screens/tor_render_test.dart`. Not done: iOS backup
  exclusion of the Tor dir, onion endpoints, per-vault circuit isolation.

## Illustrations

- **Brand decisions and research**: `docs/brand.md` (2026-10-01: Verdigris palette,
  Patina icons, Seam app icon, light vault card in light mode, gold = money). Redo or
  extend them with the project skills `.claude/skills/brand-palette` (palette + icon
  style; `oklch.js derive <hue>` prints tokens and contrast) and
  `.claude/skills/logo-design` (marks; `marks.js proof out.html`). Implemented
  2026-10-01; `app/lib/src/core/theme/primitives.dart` is generated by
  `node scripts/brand/primitives.js` (edit the generator, not the file).
- App art (onboarding and more) is original SVG generated by Python: guide and recipe in
  `docs/illustrations.md`, code in `scripts/illustrations/` (`zafe_art.py` palette/toolkit,
  `scenes.py` scenes, `scene_template.py`). Never derive from Vizor's art.
- Preview (regenerates, renders through flutter_svg to `app/build/illustration_preview/`):
  `scripts/illustrations/preview.sh [scene]`. In-app screenshots (wipes app data):
  `scripts/illustrations/device-shots.sh`.
- flutter_svg ignores/breaks `<pattern>`, `<polyline>`, `<line>`, filters: the toolkit bakes
  texture fills into geometry; always check renders with `preview.sh`, not a browser.
- The art palette in `zafe_art.py` mirrors `core/theme` ("Verdigris": tinted neutrals,
  `brand` keys = members/brand, `#00736C` family in light mode; gold = keys/funds); `sky1`
  must equal `background.window` (#080B0B / #F1F5F5), and `app/tool/illustrations/
  preview_test.dart` hardcodes the window colours too: update all three together (plus
  `app/tool/brand/render_test.dart`, `emblem_render_test.dart` and the launch colours).
- **Animated art** (2026-10-16): a scene the app animates lists its layers in `scenes.py`
  `LAYERS`; `scenes.py <scene>` also writes `assets/illustrations/layers/<scene>_<layer>_<theme>.svg`
  on the scene's canvas (each moving part drawn in its rest place). The widget stacks them, each in
  a `RepaintBoundary`, and only transforms them (no re-rasterizing). First: `KeyShardsArt`
  (`key_shards`, the key screen): the shards drift, then close into the key's outline when keygen
  finishes and the backup prompt follows. They **stop short of touching** (`_gap`): the whole key
  never exists, so the art must not show it whole. Offsets are shared with `SHARD_OFFSETS`. Second:
  `SentArt` (`sent_slot`, layers bg/glow/fore/coin) + `CoinFountain` (2026-10-16, after researching Google
  Wallet's M3 payment animation, CRED's "Paid" check, Apple's motion guidance): **the climax is at the status
  badge**, the one focal point (`MobileTransactionProgressBadge`: the sending circle squeezes, the gold circle grows
  from the centre with a spring, the check draws in, a thin gold wave; a wide filled ripple flooded the screen and a
  teal-to-gold colour fade went through olive). `successEffect` (the fountain) sits behind the badge: coins tumble
  face-on (|cos| squash, small tilt: tilted perspective coins read as leaves), launched low and wide so they fall away
  past the words, not back up into the vault. Coins on trails move with `u` **linear in time**: a quadratic curve
  walked steadily is a true gravity arc, easing made them less physical (and bunched them in the slot). While sending,
  the slot breathes; once sent no coin leaves, those in the air fade within 0.6 s (a phone recording showed them
  drifting on through "Sent!", pulling the eye from the badge) and the light dims. Words change after the
  pop (`AnimatedSwitcher`, halves never overlap), in a block that never shrinks (`_KeepTallest`) so the badge never
  moves. Review whole-screen frames: `flutter test tool/screens/sent_moment_render_test.dart`. Third: `WelcomeArt`
  (`welcome_vault`, 2026-10-16, after several rounds): the still already tells the 3-of-5 story, so **only the hand
  wheel moves**, worked by hand: uneven pushes (20-50 deg over 0.6-1.1 s, a 7% give-back), uneven pauses
  (sometimes two pushes close together), nothing drawn between them. The gold blooms in place now and then (2.3 s:
  0.9 up, 0.8 held, 0.6 down; ~15% brighter, measured; every ~7.5 s), after Vizor's welcome video (chest glow ~2.3 s
  of light every 3.8 s at +21%, measured from `assets/animations/mobile_welcome.mp4`). Tapping the door
  (`WelcomeDoorTap`, a translucent target above the screen's scroll view, which otherwise takes every touch) gives a
  full turn with a haptic tick. Rejected along the way: a 30 deg nudge every 7 s (unnoticed), a looping 3.2 s unlock
  with keys, arc, wheel, bolts, shake and ripple (too much), a band sweeping across the door (loud and fast even at
  4.5 s / 8%). The hero's gradient overlay is `IgnorePointer`. Clamp curve inputs (`(u - a) / b` can exceed 1 by
  rounding; `Curve.transform` asserts).
- Art is also used outside onboarding: backup/export/restore banners, the sending screen's
  full-page background (`SentArt`), the Activity empty state.
- **App icon and splash** are generated the same way: `scripts/brand/brand.py` ("Seam":
  deep verdigris #004A46 tile, a block split by a Z-shaped channel into a pale #D9F5F2
  upper piece and a #51DDD2 lower piece, geometry = `marks.js` `seam()`; flat colours
  only, so iOS Tinted/Clear and Android themed icons repaint it cleanly; monochrome and
  notification versions draw both pieces in one colour) writes `app/tool/brand/svg/`, and `scripts/brand/icons.sh` renders them with
  flutter_svg (`app/tool/brand/render_test.dart`) into
  `mipmap-*/ic_launcher{,_foreground,_monochrome}.png` (the adaptive background is the
  colour `@color/zafe_icon_tile`, no bitmap), `drawable{,-night}-*/splash_{icon,mark}.png`
  (the Seam tile, 144 dp, same in both themes), iOS `AppIcon.appiconset` (full-bleed,
  alpha stripped with ImageMagick) and `LaunchImage.imageset` (light + dark). The mark's
  100-unit box maps to the adaptive icon's visible 72 dp; the 56-unit block's corners
  sit 28.5 dp from the centre, inside the 66 dp safe circle. Preview (small sizes down to 24 px, themed icon, both splashes):
  `app/build/brand_preview/brand_sheet.png`. The splash colour is `@color/zafe_window`
  (`values{,-night}/colors.xml`, also the NormalTheme window background) and the iOS
  `LaunchBackground` colour set (+ the storyboard fallback); keep them equal to
  `background.window` (#080B0B / #F1F5F5). Android 12+ splash
  attributes sit in the base `styles.xml` with `tools:targetApi="31"` (no `values-v31`).
  The splash follows the app's theme setting, not just the OS: `AppThemeHost` sends it
  over `xyz.zafe/window_appearance` (`setBrightness`), and on Android 12+ `MainActivity`
  calls `UiModeManager.setApplicationNightMode`, which the OS persists for the next
  launch's splash. Android < 12 and iOS (no handler yet) still follow the OS theme.
  Check resources without a Gradle build: `aapt2 compile --dir res` + `aapt2 link` against
  `platforms/android-36/android.jar`.
- **Small phones and large text** (checked 2026-10-16): `flutter test
  tool/screens/size_sweep_test.dart` renders the newer screens and sheets at 320x568 to
  412x915 and text scales 1.0/1.3/2.0 and fails on any overflow; on the emulator use
  `adb shell wm size 720x1280 && adb shell wm density 320` (360x640 dp) and `adb shell
  settings put system font_scale 2.0` (reset with `wm size reset`, `wm density reset`,
  font_scale 1.0). A debug build records every overflow in `files/diagnostics/crashes.log`
  (`adb shell run-as xyz.zafe.zafe cat ...`), which also covers capture-protected screens.
  Rules learned: `AppButton` labels shrink to fit a bounded width; `MobileListRow` puts the
  value under the label at text scale >= 1.5 (or when the row is too narrow); sheets with prose use `constrainBody: true`
  and a scrolling body; full-screen layouts with spacers use LayoutBuilder +
  SingleChildScrollView + ConstrainedBox(minHeight) + IntrinsicHeight (Welcome).
- **Large screens, foldables, landscape** (2026-10-22): phones stay portrait, nothing else does.
  *Orientation*: no `screenOrientation` in the manifest (Android 16 ignores it at sw600 with targetSdk 36 anyway;
  on Android 14 it letterboxed the app on an unfolded Fold); iOS Info.plist lists landscape for iPhone too (a
  foldable iPhone's inner screen must rotate) and all four on iPad, no `UIRequiresFullScreen` (deprecated in
  iPadOS 26; resizable apps must take every window). `core/platform/orientation_policy.dart` asks for portrait
  unless max(display, window) shortest side >= 600 dp, again on every `didChangeMetrics` (Android keeps the
  activity through folds: `configChanges`). Android's `Display` is `computeMaximumWindowMetrics`, refreshed on each
  configuration change (`FlutterView.onConfigurationChanged` → `ViewUtils.calculateMaximumDisplayMetrics`); iOS reads
  `UIScreen.mainScreen.nativeBounds` once at start (`FlutterEngine.mm updateDisplays`), hence the window in the max.
  `DisplayFeature.bounds` are logical pixels (the engine divides, `hooks.dart _decodeDisplayFeatures`).
  *Layout*: every router page is wrapped in `AppColumn` (`core/layout/app_column.dart`, via `page()` in `app.dart`,
  the vault shell, the lock screen): at most `kMaxContentWidth` (560) wide, centred, on the start side of a hinge or a
  half-opened fold (`DisplayFeatureSubScreen`'s rule, so sheets open over it), clipped like a screen edge, with
  `MediaQuery` rewritten to the column (size, insets, features). Same widget tree at every size (resizing keeps
  state); a phone gets the window unchanged. Sheets cap at 560 (`constraints` on both sheet routes). Welcome lays out
  wide windows itself: art beside the content when width >= 600 and >= 1.2 x height, else art on top **scrolling
  with** the content (a fixed art under scrolling content put buttons on the picture). `OnboardingHero.sizeIn`
  caps art at 0.66 of the window height (never binding on phones), and art narrower than the window fades at its
  sides (`SideFade`). `isLargeWindow` (view shortest side >= 600) centres Welcome and the key screen's content
  instead of pinning it to the bottom. Short windows: `ZafeScreen`'s CTA scrolls with the body below 360 dp
  (measured without the keyboard, so phones keep it above the keyboard; a `GlobalKey` keeps a busy button's
  state), `MobileModalScaffold` bodies and the plain sheets scroll, the progress/Sent screen scrolls (clip off for
  the coins), `MobileListRow` stacks when the label leaves the value < 40 px, the scanner puts the camera beside its
  controls (short + wide) or scrolls (small), its frame is 70% of the shorter side, Receive's QR shrinks below 324.
  Checks: `flutter test tool/screens/size_sweep_test.dart` (phones, flip/fold cover, tablet both ways, split halves,
  fold inner both ways, a foldable iPhone's outer/inner sizes, landscape, a Surface Duo hinge, book and tabletop
  folds; fails on overflow, a button that can't be scrolled into view, a button on the hero art, text across a
  hinge) and `test/large_screens_test.dart`. Emulator: a `pixel_fold` AVD from the API 34 image (`avdmanager create
  avd -d pixel_fold`) has real fold states: `adb emu fold` / `unfold`, `adb shell cmd device_state state 1`
  (half-opened, reports the fold to Flutter). Its folded screen captures black with `screencap`; use `adb emu
  screenrecord screenshot <dir>` (always the 2208x1840 frame: rescale to the display's aspect). Two displays: pass
  `screencap -d <id>` (`dumpsys SurfaceFlinger --display-id`). A phone-size `wm size` on it letterboxes instead of
  rotating (the device ignores orientation requests). The IME inset animates in steps (636 → 841 px): wait before
  judging what the keyboard covers.
- **Screen previews without a device**: `flutter test tool/screens/home_render_test.dart`
  (vault card, Home buttons, notice card, activity rows, tab bar → `home_{dark,light}.png`)
  and `flutter test tool/screens/proposal_render_test.dart`
  (from `app/`) pumps the proposal body, payment card and signer rows with fake data (fonts
  loaded with `FontLoader`, `proposalReviewProvider` overridden) and writes
  `app/build/screen_preview/*_{dark,light}.png`. Widgets that call Rust (`vault.summary`,
  `myKeyHex`) can't be pumped, so keep screen bodies in public widgets that take plain data
  (`features/proposals/proposal_parts.dart`). To capture a sheet, wrap the whole
  `MaterialApp` in the `RepaintBoundary` and apply `AppTheme` through its `builder`
  (`welcome_render_test.dart`). Widget tests use the Ahem font, which is wider than DM
  Sans: a 390 pt view overflows button labels, so use a wider view there.
- In a fresh worktree `flutter analyze` reports errors in `rust_builder/cargokit/build_tool`
  until `dart pub get` is run in that directory.


## M1 app: built on Vizor (chainapsis/vizor-wallet, Apache-2.0), with Zafe's own look

Vizor is the reference for **architecture and components**, but the user asked (2026-09-30)
that Zafe **not look like a copy**. Zafe's visual identity, keep it when resyncing:
- **Palette ("Verdigris", chosen 2026-10-01, `docs/brand.md`)**: neutrals tinted ~1%
  toward hue 188 (`Primitives`), **verdigris** brand (`BrandPrimitives`: #51DDD2 dark,
  #00736C light), primary buttons #51DDD2 with a #001B19 label (dark) and #00736C with a
  white label (light). **Zcash gold = value**: received money, "+x TAZ", the ticker,
  success (`GoldPrimitives`, #F3BA3C / #835A00; never green: teal and green are too
  close). Warnings orange (`OrangePrimitives`, #FFAE81 / #A55115), errors rose
  (`RosePrimitives`, #E96CAD / #A82571). Window #080B0B / #F1F5F5. The vault and payment
  cards use `colors.vaultCard` (`AppVaultCardColors`): ink #091312 in dark mode, a light
  #E0F3F1 card with a line border in light mode (user's decision), dial drawn in the
  accent (70% in light). Earlier: "Signal" lime + charcoal (2026-09-30), jade + slate.
  **Token names follow the three colour jobs** (renamed 2026-10-01): value =
  `text.value`, `icon.value`, `background.valueAlpha` (were `positiveStrong`/`success`,
  `utilitySuccessAlpha`); errors = `*.destructive*` (the `utility` prefix is gone:
  `background.destructiveSubtle/Strong/Alpha/AlphaSubtle`, `border.destructive/
  destructiveSubtle`); `background.darkCard` / `text.darkCard` (were `homeCard`: the
  dark QR card, not the vault card); `sync.{text,textError,glow}` only. Unused gold
  leftovers (`utilitySuccessSubtle/Strong`, `border.utilitySuccess/utilityPositiveStrong`,
  `text.success`, `sync.textSyncing/lightSuccess/lightError`) were deleted.
  Material widgets use `core/theme/material_theme.dart` (was `legacy_material_theme`,
  Vizor's gray/green scheme): its `ColorScheme` and text selection come from `AppColors`,
  body font DM Sans.
- **Type**: Space Grotesk (display, amounts, titles), DM Sans (body), JetBrains Mono (codes).
- **Shapes**: rounded-rectangle buttons (`zafeButtonRadius`: 14 / 10 / 8), card radius 20,
  rounded-square avatars; activity icon tiles are circles (Patina).
- **Icons ("Patina two-tone", docs/brand.md §4)**: **every** `AppIcons` name renders
  Phosphor (MIT) SVGs from `assets/icons/patina/<name>{,_active}.svg` (generated by
  `scripts/brand/patina_icons.py`, which also keeps the licence in
  `assets/icons/licenses/`): black = outline in the icon colour (thickened to 1.7 px),
  magenta = closed shapes at 38% (`PatinaColorMapper`): the brand accent, or the icon's
  own colour when that is a semantic one (warning, error, value; `patina:` overrides);
  `AppIcon(active: true)` = the solid fill (current tab, a payment that needs your
  vote). Line icons (x, check, carets, plus, minus, arrows) use Phosphor's regular
  weight, no fill (`LINE_ONLY`). Exceptions: `AppIcons.marks` (the Zcash currency
  glyph, `assets/icons/zcash_currency.svg`, tinted) and the code-drawn `loader`. Add an
  icon to the script's `MAP` and to `AppIcons` together; unused icons are deleted, not
  kept. Contact sheet: `flutter test tool/screens/icons_render_test.dart` →
  `build/screen_preview/icons_{dark,light}.png`.
- **Signature pieces**: home "vault card" (`BalanceCard`: safe-dial rings + accent glow,
  signer-dot threshold strip); the same card for payments (`PaymentCard` in `core/widgets/mobile/zafe_detail.dart`,
  proposal page and send review); signers as `SignerRow`s with key-derived rounded-square
  `SignerTile`s (hue + mirrored 5x5 pattern from the key; "You" outlined in the brand accent) and
  `ApprovalDots` for votes; pushed pages use a boxed back button with a left-aligned 24 px
  title. Icon: Seam on a deep verdigris tile (`scripts/brand/`).
- **No Vizor references in code comments** (the user asked); attribution stays in `NOTICE`
  and the licenses page.
- `ZAFE_FORM_FACTOR` defaults to `mobile` (the phone app previously used desktop tokens
  because no build passed the flag).

What still follows Vizor:
Flutter (pinned **3.47.7** via fvm since 2026-10-22, was 3.41.6) + `flutter_rust_bridge` **2.11.1** + Rust core; Riverpod
+ go_router; `flutter_secure_storage`; design tokens with Desktop/Mobile sets selected at
build time by a `--dart-define` form-factor flag; sentence-case copy; Rust API surface
limited to primitives/flat structs (complex types stay behind it); bootstrap snapshot before
the first frame; broadcast-before-store for PCZT sends. Zafe's vault account is exactly
Vizor's Keystone account shape (UFVK-only, external signer), with FROST instead of a device.
Keep attribution/NOTICE for anything copied from Vizor.

Upstream is **github.com/chainapsis/vizor-wallet** (not the stale `valargroup` mirror the
first study used). Detailed reference (tokens, components, screens, bridge setup):
`docs/vizor-reference.md` (§11: resync notes). `lib/src/core` was resynced to upstream
`4bff2e7`; check upstream for newer work and resync file by file, re-applying Zafe edits
(AppButton semantics, `xyz.zafe/*` channels, no Vizor background PNG in the progress screen).
Upstream features to borrow later: settings screens, address book. (Tor is done: see
"Tor" under Dependency gotchas.)
Learned while studying it:
- **Copy** architecture, tokens, component specs, screen structures, and the Keystone
  signing UX (it starts proving in the background while the external signer works — do the
  same while FROST round 2 runs). Keep Apache-2.0 attribution + a modification notice.
- **Do not copy** Vizor's knight/castle illustrations and profile pictures (brand identity, no
  documented origin), the Vizor name/wordmark, `com.keplr.vizor` bundle IDs or
  `com.zcash.wallet/*` channel names.
- **Fonts** are OFL 1.1 static instances from Google Fonts (`fonts.googleapis.com/css2`
  without a browser UA returns TTF URLs); bundle each family's OFL text in
  `assets/fonts/licenses/` and register it in `main.dart`.
- **Improve on Vizor**: encrypt the wallet DB (SQLCipher, spec §14), typed errors across the
  bridge instead of substring matching.
- FRB: mark cheap calls `#[flutter_rust_bridge::frb(sync)]` (otherwise Dart gets a Future).
  `flutter_rust_bridge_codegen generate` works without `cargo-expand` (it only warns).
  The bridge crate `app/rust` (`rust_lib_zafe`) is a workspace member so it shares pins.
- App layout: `lib/src/core/` is copied Vizor code (keep in sync with NOTICE); Zafe code is
  `lib/src/{providers,features}`, `lib/src/app.dart` (GoRouter + redirect on vault state),
  `lib/main.dart` (RustLib.init → `VaultBootstrap.load()` → ProviderScope override).
  Secrets (identity, invite, key material) live in `flutter_secure_storage` via
  `core/storage/zafe_secure_store.dart`. **Its calls can get no reply** on Android when
  a background engine (WorkManager/FCM check) uses the storage at the same time: every
  call goes through `_PatientStorage` (10 s timeout, one retry); never call the plugin
  directly. The **network** is compile-time (`ZAFE_NETWORK`,
  default regtest, `core/config/network_config.dart`). Relay/lightwalletd **defaults**
  come from the build: presets per network (`regtest` local http; `test`/`testnet`:
  zec.rocks TLS lightwalletd + a placeholder relay `https://relay.zafe.invalid` until one
  is deployed, shown as "Not configured" in Settings), overridden by the dart-defines
  `ZAFE_RELAY_URL` / `ZAFE_LIGHTWALLETD_URL`. Dart const expressions can't read fields of
  const objects, hence the parallel consts. **At runtime** the user can override either
  URL in Settings (`features/settings/endpoint_sheet.dart`): `core/config/endpoints.dart`
  (`ZafeEndpoints`, `checkEndpointUrl`: https required except localhost/127.0.0.1/
  10.0.2.2 on regtest; path/query refused; typed ports kept, since Dart's `Uri` drops a
  default `:443`) stores them in prefs keyed per network (`zafe_relay_url_<net>`,
  `zafe_lightwalletd_url_<net>`; saving the default removes the key). The sheet tests
  the URL first (bridge `api/endpoints.rs`: `check_relay` = `GET /health` must answer
  `ok`; `check_lightwalletd` = `GetLightdInfo`, network must match). **Never use
  `kZafeRelayUrl`/`kZafeLightwalletdUrl` directly**: UI code reads
  `ref.read(endpointsProvider)` (from the bootstrap), background checks use
  `ZafeEndpoints.fromPrefs(prefs)` after `prefs.reload()` (prefs cache per isolate). A
  relay change re-registers the push token (`reregisterPush`). All members of a vault
  must use the same relay.
- **Site download link**: `site.yml` sets `ZAFE_DOWNLOAD_URL` to `https://github.com/zafe-cash/zafe/releases` (the testnet APKs are pre-releases, so `/releases/latest` 404s).
- **Bridge errors are typed**: API functions return `Result<T, ZafeError>` (`api/error.rs`,
  `kind` + `message`); Dart maps `ZafeErrorKind` to copy in `core/errors/zafe_error_copy.dart`.
  FRB treats a `type Result<T> = ...` alias as **anyhow** — always write
  `Result<T, ZafeError>` in public signatures or the typed error silently disappears.
- FRB `StreamSink<T>` gives Dart a `Stream` (used for send progress). A streaming
  function's returned `Err` never reaches the Dart listener (unhandled exception), and
  `sink.add_error(ZafeError)` arrives as an undecodable `AnyhowException`: report failures
  as a normal event (`SendStage::Failed` + `error: Option<ZafeError>`). The generated Dart
  `ZafeError` has no useful `toString`; log with `describeError`. Keep a plain-callback
  twin marked `#[frb(ignore)]` (`send_with_progress`) so Rust tests can drive it.
- **New in-vault routes must be added to the `inVault` list in `app.dart`'s redirect**,
  or opening them bounces to `/home` with no error (hit with `/scan-recipient`).
- **Vault tabs** (2026-09-30): `/home`, `/activity`, `/signers`, `/settings` are the
  branches of a `StatefulShellRoute.indexedStack` (`features/home/vault_shell.dart`,
  Vizor's floating `AppMobileTabBar`, `NoTransitionPage`); switch with `context.go`,
  never `push` (pushing a tab root stacks a second copy). Every other in-vault route is
  top-level, so `push` covers the tab bar. Tab screens keep ~112 px bottom padding for
  the floating bar and use `MobileTopNav.back` without `onBack`. Home stays mounted
  while another tab shows (indexed stack), so its 15 s poll keeps running.
- **Vault emblem** (`features/vaults/vault_emblem.dart`): Home's avatar and the
  switcher show one of 8 drawn motifs (dial, keyhole, peaks, waves, coins, gem, sun,
  orbit) on a dark tile in one of 6 Verdigris/gold/bronze palettes, both picked by an
  FNV-1a hash of the vault id (keep the palette count and order), so every
  member sees the same picture. Preview: `flutter test tool/screens/emblem_render_test.dart`
  → `app/build/screen_preview/vault_emblems.png`.
- **Cached balance** for the switcher lives in secure storage (`ZafeSecureStore.balance`,
  `zafe_vault_<id>_balance`, removed with the vault), not in `summary.json`; writing the
  summary drops the plain-text balance older builds stored there.
- **Vault name** is the creator's (signed in the descriptor); a member can rename it
  on this device only (`core/storage/vault_name.dart`, `<vaultDir>/vault_name.txt`,
  `vaultNamesProvider`, `activeVaultNameProvider`). Show `activeVaultNameProvider`,
  not `summary.name`; background checks use `VaultName.display`. Not in backups yet.
- **Android app category**: the manifest's `android:appCategory` has no finance value
  (game/audio/video/image/social/news/maps/productivity/accessibility only), so launchers
  that group apps (Nothing) file a sideloaded APK under "Other"; a Play Store listing's
  category is most likely what puts it under Finance (unverified).
- Flows: `/send` (recipient → amount → review → "Propose payment"), `/received/:txid`
  (money received), `/proposal/:id`
  (independent check on this device, votes, approve/reject, "Collect signatures & send"),
  `/proposal/:id/send` (Vizor's transaction progress screen; the send lives in
  `ProposalsNotifier`, so leaving the screen doesn't stop it), `/activity` (all payments).
  Home polls every 15 s (60 s while the live watch is connected, see "Live activity"):
  proposals refresh + answering signing requests, then wallet sync.
  Wallet DB access is serialized by `wallet_lock()` in the bridge.
- **Received payments** (`wallet::VaultWallet::received_payments`, bridge
  `api/received.rs` `list_received`, `providers/received_provider.dart`): `WalletDb`
  doesn't expose its connection, so it opens a second **read-only** rusqlite connection
  (path kept on `VaultWallet`) and queries the `v_received_outputs` /
  `v_received_output_spends` views: non-change outputs of transactions that spend no
  vault note, `tx_index = 0` = coinbase, block time from `blocks`. Must run under
  `wallet_lock()`. The provider reloads whenever the active vault's `Balance` changes
  (every sync bumps the height). Activity rows are `ActivityItem`s
  (`features/proposals/activity_feed.dart`, `mergeActivity`): pending receipts first,
  then by time. The notification snapshot (`seen.json`) now also holds `rx:<txid>` keys
  plus an `rx:*` marker; without the marker (older snapshots) receipts are recorded but
  not announced, so an upgrade doesn't replay history. `recordSeen(id, proposals,
  received:)` merges (a `null` list keeps that kind). Notification payload
  `vaultId:rx:<txid>` opens `/received/<txid>`.
- **Pending receipts: the mempool watch** (`zafe_core::mempool::watch`, bridge
  `api/mempool.rs`, app `providers/mempool_watch_{policy,provider}.dart`). Block sync
  never sees unmined transactions, so while the app is in the foreground the watch reads
  lightwalletd's `GetMempoolStream` (whole mempool, never a txid lookup), parses each tx
  at tip + 1, trial-decrypts it with the vault UFVK **without touching the DB**, and only
  for vault transactions calls `store` on a blocking thread, which takes `wallet_lock()`,
  opens the wallet with its key (`VaultWallet::open`, encrypted path) and runs
  `store_mempool_tx` (`decrypt_and_store_transaction`, mined height `None`). The lock is
  never held while waiting on the stream. lightwalletd closes the stream at every block:
  reconnect after 1 s; real errors back off 1, 2, 4 ... 30 s (reset once a stream
  delivers). Handled txids are remembered (bounded) since each reconnect resends the
  mempool; a failed store is not, so it is retried after the next block. Lifecycle: one
  watch per process. `begin_mempool_watch()` (sync) returns an id and makes older ids
  stale; `watch_mempool(id, ...)` spawns on the bridge runtime, returns at once and runs
  while its id is current (cancel polled every 100 ms, also mid-connect/read/sleep);
  `stop_mempool_watch()` (sync) bumps the id. The app takes the id **before** its awaits,
  so a stop during setup can't be lost. Events: `Connected`, `Stored { txid }` (the app
  reloads `receivedProvider`), `StoreFailed`, `Disconnected { retry_in_secs }`, `Failed`
  (setup error; the stream then closes). `MempoolWatchController` keeps one watch matching
  `mempoolWatchTarget(foreground, activeId, synced = balance != null, lightwalletdUrl)`
  and restarts one that ended by itself after 10 s; `ZafeApp` keeps the provider alive.
  Foreground = `resumed`/`inactive`/unknown. Background checks don't watch (they sync).
  `ReceivedNotifier.refresh` now queues one more read if called while loading.
  **The balance counts it too**: `get_wallet_summary` already includes unmined receipts (no
  mined height = pending spendability), but only once a sync recomputes it, and with no new
  block the poll skips the sync: a `Stored` event marks the wallet dirty and syncs (2026-10-22).
  Don't add unmined receipts on top of the summary: tried in 0.1.20, it counted them twice.
- `VaultWallet::create` does all network calls **before** creating the DB file; the bridge
  also deletes a DB with no account (`VaultWallet::exists`). Previously the first sync with
  lightwalletd down left an empty DB that failed forever ("expected one account, found 0").
- Vizor's `AppButton` used an onTapUp-only detector (no semantics tap action); Zafe wraps
  it in `MergeSemantics(Semantics(button, enabled, onTap, Focus(...)))`. `Focus` must stay
  **inside** the merge (outside, it adds an unlabeled focusable node and the label reads
  as a separate node); `test/app_button_semantics_test.dart` guards it. Use
  `expand: true` inside `Expanded` rows.
- **Seat moves / share repair** (spec §10.1, §10.4.2; `zafe_core::repair`): a signer who
  lost their phone and has no backup gets their seat moved to a new phone. Membership is
  now **dynamic**: `VaultState.descriptor` is the *current* membership (seat moves applied),
  not the signed original; `node::load_log` checks the `Created` entry against its own
  descriptor and every later entry against the membership as of that entry (so the
  creator can be replaced). Anything membership-related must use the replayed state
  (`members_by_pk(state)`, `frost_id_of(&state.descriptor, ..)`), never
  `material.descriptor` (which the app refreshes from `ProposalList.updated_material`, and
  `load_state` only compares on group key + UFVK). Flow: new phone shows a recovery code
  (`RecoveryRequest`); each approving co-signer runs the **safety words** check with it
  (below), then logs `ReplaceApproval` (signature over
  `relay::ReplaceApproval`, reused for the relay), at t the seat moves (votes/shares/groups/
  name re-keyed; interactive approvals voided; pool + backup attestation dropped); the
  relay swaps keys on `/v1/mailbox/replace` with t signatures (t recorded by the creator via
  `/v1/mailbox/threshold`, called by `node::create_vault`; 0 = can't move);
  `repair::help_repairs` (run by the bridge's `list_proposals`, CLI `zafe seat repair`)
  does the first t approvers' RTS rounds (own deltas saved in `<state>/repair/<index>.delta`
  before sending, `.done` after the sigma); `repair::try_recover` (bridge `check_recovery`,
  CLI `zafe recover --wait`) finds the vault via `/v1/mailboxes`, reads the log with a
  helper's log key, requires every helper's sigma with identical vault data, and checks
  the verifying share, group key and UFVK. `load_state` boxes `load_log` (an unboxed future
  overflowed rustc's query depth in `send_with_progress`). App: Welcome → "I'm already
  in a vault" → "No, ask my co-signers" (`/recover`; "Yes, restore it" opens `/restore`, pending identity in secure storage
  `zafe_recovery_identity`; once done the member must confirm the vault address's last 8
  characters with a co-signer (`VaultCheckBody`), because the new phone can't know which
  vault to expect and a dishonest relay could present one of its own; then
  `addRestoredVault` and `/backup-prompt`); Signers tab →
  "Replace a lost phone" (`/replace-signer`, extra `(oldKey, code)` for a pending move) and
  `SeatMoveCard`s. Tests: `crates/zafe-core/tests/repair.rs` (creator replaced, same share,
  signs with a co-signer), `tests/vault.rs` replay rules, `app/rust/tests/repair_bridge.rs`
  (bridge calls, no Docker). Preview: `flutter test tool/screens/repair_render_test.dart`.
  Stalled repairs: `Replacement { helpers, attempt, done }`; any non-helper member can
  `repair::retry_repair` (bridge `retry_repair`, CLI `zafe seat retry`, Signers tab
  `RepairCard` "Help instead of X") to start attempt + 1 with itself in a helper's place;
  helper state files are `<index>-<attempt>.{delta,done}`; the new phone logs `RepairDone`
  from the bridge's `list_proposals` (CLI `zafe recover`), and `ProposalList.repairs` lists
  unfinished ones. QR: `/scan-recovery` (`ScanRecoveryScreen`) from the Replace screen.
  Notifications: pending moves are announced once (`mv:<old>:<new>` keys + `mv:*` marker in
  `seen.json`; not to the old key or members who approved; tap opens Signers), and
  background checks save `updatedMaterial`. Live-tested on the emulator
  2026-10-01 both ways (app as helper, app as the new phone) with CLI members
  (`zafe recover`, `zafe seat words|approve|repair`). Tips: `/recover` is a SecureScreen
  (screenshots are black: read it with `agent-device snapshot`); the code is not shown as
  text, so "Copy code", then paste it into Join's field to read it.
- **Seat recovery safety words** (2026-10-15, audit M2; replaced an 8-digit code of the new
  keys, which a swapped recovery code could match in seconds): commit, challenge, reveal
  through the relay (`POST /v1/recovery/put|read`, `RecoveryPut` signed; new routes, no
  `RELAY_API` bump, table `recovery_values` added by `CREATE TABLE IF NOT EXISTS`, so no
  `RELAY_DB` bump; first value per (subject, signer, kind) stays, at most
  `MAX_RECOVERY_CHALLENGES` = 16 challenges per new phone, pruned after 7 days; the relay
  stores the canonical signed bytes, clients verify). The new phone
  (`repair::recovery_checks`, run by the bridge's `check_recovery` poll) publishes a
  commitment to its secret, reveals it once a challenge exists, and lists the words per
  co-signer key (`RecoveryProgress.checks`). A co-signer (`repair::new_phone_words`, bridge
  `new_phone_words`, polled by the Replace screen) challenges with a value derived from a
  signature over (subject, commitment), refuses a commitment that changed after it
  challenged, checks the reveal and returns 4 words (`sas::recovery_words`).
  `approve_replacement` takes the confirmed words and recomputes them (reading them and the
  log at once). The new phone's `/recover` screen checks every 2 s: co-signers wait on it. Both secrets are
  signature-derived (nothing stored). Every approver does its own check, also when
  approving from the log's pending move. CLI: `zafe seat words <code>` then `zafe seat
  approve <old> <code> --words "..."`; `zafe recover` prints the words as they arrive. An
  older relay answers 404 → "the relay doesn't support safe seat recovery yet".
- **Backup health** (spec §12.2): `VaultEvent::BackupVerified { epoch, at }` (only for the
  current descriptor epoch; latest per member in `VaultState.backups`), written by
  `node::attest_backup` (once per epoch; bridge `attest_backup`, CLI `zafe backup`).
  `export_vault_backup` decrypts the backup it just made before returning it. The app
  sets the local `summary.json` `backedUp` only once the file was shared (share result
  not `dismissed`; `unavailable` counts) or the text copied, and `ProposalsNotifier`
  attests whenever `backedUp` is set but the log's `ProposalList.backed_up` lacks this
  member (covers exports, restores and devices backed up before attestations; once per
  vault per session). Signers tab: `BackupHealthCard` + `BackupLabel` per row (preview
  in `tool/screens/protection_render_test.dart`).
- **Signer names** (`core/storage/member_names.dart`, `names.json` per vault) travel as
  `Vec<SignerName>` (`api/names.rs`) to `export_vault_backup` / `export_history_csv`
  and back from `import_vault_backup`. Background checks read `names.json` themselves
  (`MemberNames.read`, no providers in that isolate) and pass `names:` to `vaultUpdates`.
- **Shared names**: each member can also name *themselves* in the log
  (`VaultEvent::Name`, `node::set_name`, bridge `set_my_name`; only the author, trimmed,
  ≤ 32 chars, no control chars, empty clears). `ProposalList.shared_names` carries them;
  display uses `MemberNames.merge(shared, local)` (`signerNamesProvider`): this phone's
  label wins, then the shared name, then the short key. Tiles stay key-derived, so a
  member calling themselves "Bob" still shows their own tile and key. "Your name" is the
  sheet behind your own row on the Signers tab; the CLI has `zafe name`.
- **Name in setup** (2026-10-16): Create and Join take "Your name" (`core/storage/my_name.dart`, last one
  in prefs `zafe_last_my_name` prefills); it can't be published before the vault exists (a `Name` is a log
  entry), so it waits in `<vaultDir>/my_name_pending.txt` and `ProposalsNotifier._publishPendingName` sends it
  on the first refresh with material, unless a shared name for this member exists already.
- **CSV recipients import** (`features/send/recipients_csv.dart`): pure parser with the
  bridge validators injected (`checkAddress` for `kZafeNetwork`, `parseZec`,
  `memoLength`), so it's unit-tested without Rust. All-or-nothing.
- **Viewing key** (`/viewing-key`, in `inVault`, SecureScreen): bridge
  `vault_viewing_key(material)` derives the UFVK and refuses if it differs from the
  descriptor's `ufvk` (`Verification`).
- Pushed-page titles are 24 px and left-aligned (room for ~20 characters); keep them short.
- **An icon leading a row of text** goes in `core/widgets/line_icon.dart` `LineIcon(size:, line: <first line's
  style>)`: it centres the icon on the first line (scaled with text size). A bare `AppIcon` in a
  `CrossAxisAlignment.start` row sits low next to a 17 px title (2026-10-22, all ten cards moved).
- **Copy and layout rules (user, 2026-10-16)**: minimal text: delete lines that restate the screen,
  show exact values (bridge `estimate_fee`, never "at least"), explanations only in help sheets.
  Check UI on a 360x640 phone at 1.0 and 2.0x text (`adb shell wm size 720x1280; wm density 320;
  settings put system font_scale 2.0`, reset after) and run `flutter test
  tool/screens/size_sweep_test.dart` (fails on any overflow); test proposer **and** approver flows.
  No dead ends: "Sent!" returns Home by itself (`kSentHoldTime`), reject/cancel/non-final approve
  go back; Tx IDs open ZecBlock (`core/config/block_explorer.dart`).
- **Actions and feedback (2026-10-16, polish pass)**: a tap that takes a moment or can fail
  uses `core/widgets/action_button.dart` `ActionButton` (spinner inside, optional
  `busyLabel`, `doneLabel` check, shake + inline error via `zafeErrorMessage`; throw
  `ActionCancelled` to back out silently, `UserFacingError` for app-made messages). **There
  are no toasts** (the widget was deleted 2026-10-16): the screen change or the button is the
  feedback; `CopyButton`/`TxIdRow` turn their icon into a check. Events not caused by a tap (a
  payment link expired, sync moved server, a history export failed) go to `appNoticesProvider`,
  shown as dismissible cards by `AppNoticeCards` on Home, Welcome and Activity. Unlock failures
  are shown by the system prompt itself. Switches use
  `AppSwitch` (with `busy`), small exclusive choices `AppSegmented` (icons). Buttons press
  to 96% (`AppButton`). Payments that need your vote open as a request
  (`PaymentRequestView`, Approve/Decline pinned; the request stays on screen through
  "Approved" while it leaves: `_holdRequest`). Auto-send is always on (the toggle is gone;
  `SendPrefill` has no `autoSend`). Confirmation sheets only for what can't be undone
  (remove vault, cancel payment, abandon setup); Tor is a row switch, theme inline, backup
  "Later" just leaves.
- **The proposer approves when proposing** (2026-10-16, user's call after the testnet dry run, as Safe does):
  `send_screen._propose` runs bridge `propose_and_approve` under one unlock ("Propose and approve this payment"):
  `node::propose_loaded` then `node::approve_after_propose` with the log state the proposal left (2026-10-17, one
  log read fewer), returning the proposal list so the page opens at once; a failed approval comes back as
  `approve_error` and leaves the payment open for this member's approval (the request page then says "You proposed
  sending"). Proposing reads lightwalletd's tip and branch in one `GetLightdInfo` (`tip_on_current_branch`)
  alongside the log, before the wallet lock (`propose_inner`). Before, the proposer's own payment came back as a request with Approve/Decline. The CLI still
  proposes and approves separately.
- **Needs you** (Home, 2026-10-16, `features/home/needs_you.dart`): payments where `needsMyAction`
  (`proposal_status.dart`; the same rule counts notifications) and seat moves this member can approve, as
  `RequestCard`s (amount first with a gold ticker, then who and the message; a tonal full-width button
  naming the decision: "Approve or decline" opens the request, never approves from Home since the card
  lacks the address; "Send now" sends after the unlock; "Compare safety words" for seat moves) in an
  `AnimatedKeyedColumn` (cards grow in and collapse out, 250 ms); from 3 requests they stack (`_FanStack`:
  the first card in full, two edges under it, "N waiting"); tapping the top card or the edges fans them out
  (300 ms), leaving for a payment folds it back. Decline is optimistic (`declinesProvider`: the card leaves on
  the tap, a failure brings it back with the reason). Test: `test/needs_you_stack_test.dart`. Tip: phone
  screenshots are scaled to 900 px wide; multiply by the stated factor before `adb shell input tap`, above every notice except "Money left
  without approval"; they leave Recent activity. Nothing else (no badge, no "waiting for others" group): the user
  wants attention asked for only when the member must act.
- **Privacy mode is app-wide** (`privacyModeProvider`, persisted in prefs, read in the
  bootstrap): every amount goes through `amountWithTicker(text, hide:)`. Vizor's
  `hideAmountIfPrivacyMode` only appends the unit to the *mask*, so passing a bare amount
  drops the ticker when visible. Payment rows use a 3-star mask (Vizor's activity rows).
- **Payment sounds** (`docs/sounds.md`): one three-strike signature (E6, B6, E7) cut per
  moment: approve, ready (this approval completed the signatures), sent, received,
  failed. Synthesized by `scripts/sounds/sounds.py` (deterministic; regenerate the app's
  `res/raw/pay_*.ogg` with `scripts/sounds/build.sh`, never edit the Oggs). Played by
  `PaymentFeedback.play(moment, sound:)` → `xyz.zafe/payment_feedback` (`MainActivity`:
  SoundPool, sonification usage, silent unless the ringer is normal; haptic taps in the
  sound's tempo, kept in sync with `PaymentFeedback.taps`). Setting "Payment sounds"
  (`paymentSoundsProvider`, `zafe_payment_sounds`, default on, read in the bootstrap)
  mutes the sound only. Received plays from `ReceivedNotifier.refresh` for new txids that
  are unmined or ≤ 2 confirmations, in the foreground, never on a first load. iOS: no
  handler (haptics only); needs CAF/M4A copies. Payment sounds are a signature, not UI
  feedback: keep them clean, short, open intervals (the user rejected chimes, bass
  "tactile" thuds and bright major-third runs).
- **Balance card + dollars** (2026-10-06, after Vizor): one total (pending included; what's
  pending shows as "Pending" in activity rows, not as notes on the card) and a dollar line
  `fiatText` from `zecPriceProvider`. **Mainnet builds only** (`kShowsFiat`: test coins
  have no value; Vizor does the same), so testnet/regtest never fetch a price. The price
  comes from Rust (`zafe_core::price::zec_usd`, bridge `api/price.rs`) so it follows the Tor
  policy: Tor on → `zcash_client_backend` cryptex (several exchanges, Gemini trusted, own
  isolated circuit `Purpose::Price`); off → one CoinGecko request (needs a User-Agent:
  403 without). Refresh 3 min in the foreground, last price cached 1 h in prefs
  (`zafe_zec_usd_v1`). Live tests: `price::tests::live_direct_price` and the Tor live test.
- **In-app updates** (`services/app_update.dart`, `in_app_update` **4.2.5**: 5.x needs
  Flutter 3.44): Android Play installs only; checks at launch/unlock/resume (6 h apart).
  Priority comes from the release's class (`docs/releases/<version>.md`: essential = 5,
  optional = 2; set by `release-mainnet.yml` and `play-promote.yml`, never by hand: the Console
  can't set it). Priority ≥ 4 = immediate full-screen update,
  except while a payment is sending; otherwise a background download, then a Home
  "Update ready" card whose Restart is refused mid-send. Debug/sideloaded installs get a
  Play error and nothing shows. Can only be tested end to end with a Play install
  (internal app sharing or an internal testing track).
- **Secret screens** block capture: wrap a route's page in `SecureScreen`
  (`core/platform/secure_screen.dart`, counted) → `xyz.zafe/secure_screen` `setSecure`
  in `MainActivity.kt` (`FLAG_SECURE`). The first Zafe channel with an Android handler:
  `xyz.zafe/haptics` and `window_appearance` have none yet (Dart swallows
  `MissingPluginException`). Check with `adb shell dumpsys window windows | grep SECURE`.
- Settings (`/settings` tab) is app-only since 2026-10-16: Security (lock app), Privacy (hide
  amounts, Tor, diagnostics), System (theme, payment sounds, relay, Zcash server, licenses; fonts +
  NOTICE registered in `main.dart`). Vault things live in **Manage vaults** (`/vaults`,
  `features/vaults/manage_vaults_screen.dart`, opened from the vault sheet `showVaultSwitcher`: the
  open vault large, other vaults with a dot when something waits, [Manage vaults] [+]); each vault's ⋯
  (`vault_actions.dart` `showVaultActions`) has Rename, Back up, Viewing key, How it's protected, Remove
  (switching to that vault first where needed).
  "How it's protected" (`/vault-protection`, `VaultProtectionBody` takes plain data;
  preview `flutter test tool/screens/protection_render_test.dart`): the approval rule, full
  view access, what losing too many keys means, and the post-quantum caveat (spec §2.3).
- **Sync failures** (`core/errors/sync_failure.dart`, pure, `test/sync_failure_test.dart`):
  `homeSyncFailure(syncError:, relayError:)` combines the last wallet sync error
  (`VaultState.syncError`, kept while the next attempt runs so the label doesn't flicker)
  and the last vault refresh error (`ProposalsState.error`) into a `SyncFailureKind`
  (both unreachable = offline). Home's top-nav status shows `statusLabel`; tapping it
  (`MobileTopNav.onSyncTap`) opens `features/home/sync_status_sheet.dart`. Last sync time
  is `VaultState.syncedAt` / `summary.json` `syncedAt` (also written by background
  checks). Classification is typed end to end: `zafe_core::net::NetFailure` reads error
  **types** (rustls error inside `io::Error::get_ref`, `io::ErrorKind::TimedOut`,
  `tonic::TimeoutExpired`/`Cancelled`, reqwest `is_timeout`, non-transport gRPC codes =
  `Server`) into `WalletError::Remote { failure }` / `RelayClientError::Transport
  { failure }`; `wallet::check_server` (run by `sync_vault`) turns `GetLightdInfo` into
  `WrongNetwork` (mainnet vs not only: test networks report names inconsistently) or
  `ServerBehind` (tip + 3 < wallet's known chain tip); `VaultWallet::create` reports a
  birthday above the server tip + 1 as `ServerBehind`. The bridge maps them to
  `ZafeErrorKind::{Network, Tls, NetworkTimeout, ServerBehind, WrongNetwork,
  WalletDatabase}` plus `ZafeError.endpoint` (`Relay`/`Lightwalletd`/`None`). The relay
  client now has a 10 s connect / 60 s request timeout (it had none).
- **Sync status animation** (2026-10-12): the shimmer + breathing glow in `MobileTopNav` is Vizor's
  widget, but it ran an `AnimationController` (a frame per vsync, ~120/s on a 120 Hz phone) for the
  whole sync. Profiled with `flutter run --profile`/`flutter attach` + the VM-service timeline on a
  Nothing phone: every frame costs ~5 ms of raster time (Impeller re-encodes the whole screen), so a
  continuous animation held the raster thread at 70-78% of a core. Bisecting the Home scene (vault
  card, tab-bar backdrop blur, scroll fade) showed **no single widget is the cost**: with all three
  hidden a frame still cost 3.3 ms, and the shimmer/glow added only ~1 ms. So the fix is the frame
  rate, not the widget: a `Timer` steps the sweep and glow every 60 ms (~16/s,
  `_SyncStatusMotion.frameInterval`), no ticker, so the engine draws ~16 frames/s: raster ~15% of a
  core (measured), no cap needed. Any new indeterminate animation should do the same (or be short):
  Impeller has no partial repaint, so anything that animates redraws the whole screen. Tests:
  `test/mobile_top_nav_motion_test.dart` (no frame scheduled between ticks). **First-sync stall**
  (separate cause): see the wallet-key note under "Wallet DB is encrypted"; the steps before the Rust
  call time out in 45 s and a slow/stuck sync is recorded in the report as `slow-sync` / `sync` entries.
- **Sync status**: "First sync..." while the first sync of a vault on this device runs (`VaultState.firstSync`:
  no saved sync time); the
  sync sheet says "First sync" / "Syncing" while a sync runs (it used to say "Vault in sync"
  whenever there was no error). While "Use Tor" is on the status line and its glow are Tor's
  purple (`AppSyncColors.torText/torGlow`), unless Tor failed (error colour). Settings'
  version comes from `--dart-define=ZAFE_VERSION` (both release workflows pass it).
- **Secure storage plugin** (2026-10-12): `flutter_secure_storage` 11.2.0 (was 10.3.4; we have used the 10.x
  default ciphers since the first commit, so no data used the algorithms v11 dropped). Why: 10.x probed
  for old `EncryptedSharedPreferences` data on its first call in every engine (the "migrated" flag is only
  set after a real migration), which threw "decryption failed" at each start (logged by the plugin, then it
  fell back to the normal path), once per engine (app + background), on the one handler thread every storage
  call queues behind; v11 removed that backend. Upgrade needed `file_picker` 13.1.0 and `share_plus` 13.3.1
  (the plugin's Windows part needs `win32` ^6; a `win32` override breaks `flutter test`, which compiles
  `file_picker`'s Windows code on the host). `file_picker` 13: `FilePicker.pickFile()` +
  `PlatformFile.readAsBytes()` replace `pickFiles(withData: true)`. **Verified on a phone**: an app on
  10.3.4 holding a vault was updated in place (same signing key) to 11.2.0: same vault, address and signer
  key, zero `FlutterSecureStorage` errors, then a new vault (write path, keygen, first sync) worked. Repeat
  that in-place test whenever the plugin changes: it guards the key shares.
- **Strong unlock on Android** (2026-10-15, user's decision): `local_auth` always offers
  weak (class 2) biometrics such as camera face unlock, so Android uses
  `StrongDeviceAuthenticator` (`xyz.zafe/device_auth` in `MainActivity`): `BiometricPrompt`
  with `BIOMETRIC_STRONG | DEVICE_CREDENTIAL` on Android 11+, the `KeyguardManager` PIN
  screen alone below (the two can't be combined there). Error names are local_auth's, so
  `DeviceAuth.outcomeForError` maps both; iOS keeps `LocalDeviceAuthenticator`
  (`platformAuthenticator()`). Same activity: `filterTouchesWhenObscured` on the decor
  view (overlay tapjacking), and `xyz.zafe/clipboard`: `copySecret` (invite link, viewing
  key, backup text) flags the clip sensitive and clears it after 60 s if unchanged.
  `clearShareCache` deletes `<cache>/share_plus` at launch (share_plus keeps the last
  shared file, e.g. a backup, until the next share). Memos and payment-link labels go
  through the bridge's `visible_text` (hidden/bidi characters shown as U+FFFD); "Proposed
  by" and notifications add the short key after a member's name. Release workflows now
  fail without `ZAFE_LINK_HOST` (`zafe://` can be claimed by any app).
- **Unlock gate** (spec §14; `core/security/device_auth.dart` pure logic behind a
  `DeviceAuthenticator`, `core/security/unlock_gate.dart` `confirmUnlock(context, ref,
  reason:)`, `providers/device_lock_provider.dart`): approving, Send now / Collect
  signatures & send / Try again, proposing, exporting a backup and removing a vault call
  `if (!await confirmUnlock(...)) return;` before acting (or throw `ActionCancelled` from an
  `ActionButton` action). Rejecting doesn't. **Always required** (2026-10-16: the "Require
  unlock to approve" setting was removed; the prompt's reason text is the confirmation,
  e.g. "Approve this payment. It can't be undone."). Biometrics **or** device credential. No screen lock
  (e.g. the emulator) → the action goes ahead, with a permanent note in Settings; unknown errors fail closed. Background checks (push,
  WorkManager) answer signing requests and finish auto-sends **without** a prompt: they only
  act on approvals the owner already gave with an unlock. `local_auth` 3.x needs
  `MainActivity : FlutterFragmentActivity` (else `uiUnavailable` → every gated action is
  blocked), `USE_BIOMETRIC`, and iOS `NSFaceIDUsageDescription`.
- **App lock** (spec §14; `core/security/app_lock.dart` pure rules,
  `app_lock_gate.dart` `AppLockGate` = `MaterialApp.builder`, so the lock screen covers
  routes, sheets and toasts while the screen underneath stays mounted with input, focus
  and semantics excluded). Locks at launch when a vault exists and on resume after the
  "Lock app" delay (`zafe_app_lock`: off / immediately / 1 min default / 5 min, read in
  the bootstrap; weakening it needs an unlock). `appLockedProvider` holds the state.
  Gotcha: the device-credential prompt is another activity, so the app goes `hidden`
  behind it; `DeviceAuth.prompting` marks that so unlocking (or a `confirmUnlock`)
  doesn't lock the app again, and a cancelled prompt waits for a tap instead of
  re-prompting. No screen lock on the phone → the app unlocks (Settings warns). It is
  separate from the per-action unlock (`confirmUnlock`). Background engines are
  unaffected. **Recent apps**: `MainActivity` calls `setRecentsScreenshotEnabled(false)`
  (Android 13+; screenshots in the app still work) and below 13 sets `FLAG_SECURE` only
  between `onPause` and `onResume` (never clearing one a `SecureScreen` asked for:
  `secureRequested`). That flag doesn't hide the **live** tile Android 13+ shows for the
  running app, so `AppLockGate` also draws `PrivacyCover` whenever the lifecycle isn't
  `resumed` (switcher, notification shade, system dialogs, the unlock prompt). iOS:
  `SceneDelegate` adds a launch-screen cover on `sceneWillResignActive` (untested: iOS
  has never been built). Verified on the emulator 2026-10-04: blank recents card,
  cover gone on return. Emulator test: the
  AVD has no screen lock (the lock then opens at once); `adb shell locksettings set-pin
  1234`, unlock with `adb shell input text 1234` + `KEYCODE_ENTER`, and `locksettings
  clear --old 1234` afterwards. The PIN prompt is a secure window (black screenshots;
  read it with `agent-device snapshot`); one Back cancels it, a second leaves the app.
  Verified 2026-10-04: launch lock, cancel without re-prompt, "Immediately" without a
  relock loop, weakening asks for the PIN.
- **Payment links** (`zcash:`, ZIP 321; "Pay with Zcash" on a site or another app):
  Android intent filter + iOS `CFBundleURLSchemes`; `services/invite_links.dart` reads
  `app_links`' **string** stream (a payment link reaches Rust exactly as delivered) into
  `paymentLinks` (`features/send/payment_link.dart`, latest wins). `app.dart`'s
  `openPaymentLink` waits while locked, during keygen and on `/backup-prompt`, drops a
  link older than `kPaymentLinkTtl` (10 min, Vizor's rule: an old link must not turn
  into a payment on some later unlock) or with no ready vault (toast), then pushes
  `/payment-request` (exempt from the vault redirect: it picks its own vault).
  `PaymentRequestScreen`: "Check who sent this" warning, full address, the link's
  `label`/`message` shown as "(not verified)" (bridge `ScannedPayment.label/message`),
  "Pay from" showing only the chosen vault, with "Change" (a sheet of every vault) when
  there are several (the user asked not to list them all on the page), the approval rule, a funds check
  (`checkFunds`: amount + `minimumFeeZat` = 5,000 × max(2, recipients) against the
  vault's spendable balance when synced this session, else its saved total, which can
  only prove a shortfall; a vault that's short is marked "short by X", can't be picked,
  and the default moves to one that can pay; nothing is built, so the real fee can be
  higher), and Continue
  only after "I know who sent this..." is ticked; Continue switches vault and opens
  `/send` prefilled (`SendPrefill.fromLink`: review step, or amount when the link has
  none). Proposing still takes `confirmUnlock` and t approvals. Test:
  `adb shell "am start -a android.intent.action.VIEW -d 'zcash:<ua>?amount=1&label=Shop'"`.
  Preview: `flutter test tool/screens/payment_request_render_test.dart`. Verified on the
  emulator 2026-10-04: cold start through the lock, warm link, Continue → Review
  ("PAYMENT REQUEST"). Full e2e 2026-10-04: two vaults made with CLI members (B1/C1,
  B2/C2 + the app; 15 and 5 ZEC), link for 2 ZEC, picked the non-active vault, proposed,
  app + C2 approved, interactive send (C2 `respond`), recipient +2, payer −2.0001 (fee
  10,000 zats = the shown minimum). Backing out to the launcher ends the activity, so a link still
  waiting behind the lock is gone when the app is reopened from the launcher.
- **Backups** (spec §12.2; `zafe_core::backup`, bridge `api/backup.rs`, app `features/backup/`):
  `ZAFEBAK` v2 = header (Argon2id params, salt, nonce; authenticated as AEAD data) +
  XChaCha20-Poly1305 of {identity seeds, material, invite, signer names}; **never nonces**
  (nor the wallet DB key: a restore gets a new key and resyncs). v1 (no names) still
  restores. Restore writes the names to `names.json` (cleaned with `MemberNames.clean`,
  non-hex keys dropped) before the vault becomes active. Import checks
  the identity is a vault member and refuses KDF params below 64 MiB / 3 passes (or absurdly
  high). Text form `zafe-backup-v1:` + base64url. Passphrase: since 2026-10-16 the app
  always generates it (12 BIP-39 words, 128 bits; `ExportScreen`, words shown once, "Copy words" through
  `copySecret`) and the member types 3 random words back before export (the old "Suggest" filled both fields,
  so nothing checked the words were saved). Restore takes the words or an old typed passphrase (`zxcvbn` 4 still
  enforced in Rust for those), trying the input exactly, then trimmed/lowercased/single-spaced. The app prompts right after key generation
  (`/backup-prompt`) and shows a Home reminder **once the vault holds money** until `summary.json` has `backedUp` (an empty
  vault leaves it to the Signers tab's backup card, which has "Back up now" while this phone has none; 2026-10-22)
  (set by export, and by restoring). CLI: `zafe backup --passphrase …` prints the text form.
  Testing tip: this emulator has no shell clipboard; paste into a field (e.g. Settings
  search) and read it with `agent-device get text @ref` (snapshots truncate long values).
- **Multiple vaults**: `ZafeSecureStore` keeps each vault's identity, invite and material
  under `zafe_vault_<vaultId>_*`, listed in `zafe_vaults`; the active vault id is in prefs
  (`zafe_active_vault`). Each vault gets a **fresh member identity** (the relay can't link
  memberships). Per-vault files live in `ZafePaths.vaultDir(id)`: `signing/` (pass
  `await paths.stateDir(id)` to Rust), `seen.json`, `summary.json` (switcher's balance and
  pending count, written by the app and background checks). Wallet DBs stay
  `vault-<id>.sqlite` in the support dir. `VaultState` exposes the **active** vault
  through the old getters (`identity`, `material`, `hasVault`...); `ProposalsNotifier`
  rebuilds when `activeId` changes. The pre-multi-vault layout is migrated in
  `VaultBootstrap.load()`. Notification payloads are `vaultId:proposalId`; a tap switches
  vaults first. The switcher opens from the vault name on home.
- **Notifications** (`lib/src/notifications/`): the relay pushes every other member on
  each log append (content-free; FCM data message, high priority, one collapse key). A push,
  the 15-minute WorkManager task, or a one-off check a minute after the app is backgrounded
  all run `checkVaultAndNotify()`: sync, list, answer interactive requests, finish an
  auto-send this member owes, then `vaultUpdates()` (pure, unit-tested) diffs against the
  last-seen snapshot (`<appSupport>/notifications/seen.json`, a file because prefs caches
  per isolate) and shows local notifications; the app records the snapshot on every
  refresh so nothing seen in-app is re-announced. Tapping opens `/proposal/:id`.
  One check at a time (`notifications/check_lock.dart`, files shared across isolates): a check
  that finds one running leaves `check.again` and the running one repeats (at most 4 runs).
  Before 2026-10-22 it was dropped: a proposal pushed seconds after another vault's activity
  got no notification or switcher dot until the 15-minute task (seen in the 0.1.18 dry run).
  Same rule in the app: never drop a request because work is running. Wallet sync goes through
  `core/async/coalesced_run.dart` (`CoalescedRun`: calls meanwhile = one more run, forced if any
  asked); proposals use `refreshSoon` (queues) or `refreshFresh` (waits, then runs) after a change
  of this device (Tor, server, backup, a finished send), never the plain `refresh()`, which returns
  while one is running.
- **Notification icon**: `res/drawable/ic_notification.xml`, a vector of the icon's Seam mark
  written by `scripts/brand/brand.py` (`write_notification_icon`); Android uses only
  its alpha, so a full-colour launcher icon shows as a filled square. Used by
  `flutter_local_notifications` (init + `icon:`, tinted `#00736C`, also `@color/zafe_notification`) and as FCM's
  `default_notification_icon`. `res/raw/keep.xml` keeps it through release resource
  shrinking (Dart refers to it by name only).
- **FCM is opt-in per build**: drop the Firebase project's `google-services.json` into
  `app/android/app/` (gitignored) and the Gradle plugin applies itself; without it the app
  builds and relies on background checks. Relay: `ZAFE_FCM_SERVICE_ACCOUNT=<key.json>`
  (never commit it). APNs isn't implemented yet (iOS).
- **Firebase project `zafe-18c4d`** (the user's account), Android apps `xyz.zafe.zafe`
  (`1:303423821426:android:deff491a8e80f394ea14eb`) and the testnet app
  `xyz.zafe.zafe.testnet` (`1:303423821426:android:8a429ba817be26f5ea14eb`, added
  2026-10-06); one `google-services.json` lists both. Regenerate the app config with
  `npx -y firebase-tools@latest apps:sdkconfig ANDROID <app id> --project zafe-18c4d --out
  app/android/app/google-services.json`. The relay key (service account
  `firebase-adminsdk-fbsvc@zafe-18c4d.iam.gserviceaccount.com`) lives at
  `~/.config/zafe/fcm-service-account.json` (0600, outside the repo); it was created
  through the IAM API with the Firebase CLI login token (the CLI has no command for it).
  The CLI login in a non-interactive shell is two steps: `login --no-localhost`, then
  `login <code>`.
- **FCM on the emulator is slow** (~2 min from send to delivery on `google_apis`), so
  don't conclude "not delivered" too early: check `adb logcat | grep FLTFireMsg`. The
  relay logs `push: sent` at debug level (`RUST_LOG=zafe_relay=debug`).
- **Invites** (`features/onboarding/invite_link.dart`, pure and unit-tested): raw form
  `zafe-invite-v2:<hex postcard>` (~250 chars; URL-safe, asserted in `node::tests`), link
  form `zafe://join?invite=<raw>`, or `https://<ZAFE_LINK_HOST>/join#<raw>` when the build
  has the dart-define (invite in the **fragment**, so the landing site's server never sees
  it; only that exact host, port 443, no query). Both forms are always accepted; the https
  one only for the configured host. **Never default `ZAFE_LINK_HOST` to a domain we don't
  control**: that site's page script could read every invite. The setup QR and "Share link" carry the link; Join
  accepts either (typed, pasted inside a message, or scanned on `/scan-invite`). **An
  invite is a bearer credential until membership is locked**: its `join_token` lets anyone
  take an empty seat (it can't spend or see funds; the creator's member list and the
  safety number catch an intruder). Copy says to share it only with co-signers over a
  trusted channel. Links never join by themselves: they open Join prefilled, and the
  safety number flow follows.
- **Deep links**: `app_links` 7.0.0 (7.1+ needs Flutter 3.44), with Flutter's own deep
  linking **off** (`flutter_deeplinking_enabled` meta-data / `FlutterDeepLinkingEnabled`),
  otherwise go_router receives `zafe://join` itself. **App Links**: a second, `autoVerify`
  intent filter for `https://<host>/join`; Gradle decodes Flutter's `dart-defines` project
  property to put `ZAFE_LINK_HOST` in the `zafeLinkHost` manifest placeholder (unset →
  `links.zafe.invalid`, which matches nothing; a malformed host fails the build). Check the
  merge without a full build: `flutter build apk --config-only`, then
  `JAVA_HOME=~/android/jdk-17 android/gradlew -p android :app:processDebugMainManifest
  -Pdart-defines=<base64 of NAME=value>`. Landing site + `assetlinks.json`/AASA:
  `infra/site/` (README). iOS Associated Domains not added yet (iOS has never been built;
  the entitlement breaks signing without a team). `services/invite_links.dart` feeds
  `inviteLinks` and `paymentLinks` (its stream also delivers the launch link); `app.dart` opens
  `/welcome` + push `/join?invite=` (calling `beginAddVault` when a vault is active) and
  defers while keys are being made or on `/backup-prompt`. Test on a device with
  `adb shell "am start -a android.intent.action.VIEW -d 'zafe://join?invite=zafe-invite-v2:...'"`
  (quoted twice: the device shell splits it again; cold start: `adb shell am force-stop
  xyz.zafe.zafe` first).
- **Website** (`infra/site/`, Astro 7, static): a calm home page `/` (`hx-*`/`hc-*`
  classes; since 2026-10-23 four illustrated loops, each a Blender scene moving with an
  app-rendered card, `docs/site.md` §6; its only script is `loops.js`, the pause buttons), `/download`,
  `/support`, the 3D story on `/showcase` (keep heavy motion off the home page: the user's
  call, a treasury product should feel sober), and the invite page `/join`. The site's one
  server function is the iPhone waitlist (`infra/site/api/waitlist.mjs`, private Vercel Blob;
  README "The iPhone waitlist"); product truth for design work is `infra/site/PRODUCT.md`.
  **The two pages must not look alike** (round 12): home shows the app's real components on
  drawn (not 3D-rendered) scenes; the showcase has no logo intro, a fixed `light` nav (`Nav.astro`),
  a title card and a slim footer. Nav links marked `mobile` are the only ones on phones.
  Phones (2026-10-01 fixes): `body`'s `overflow-x: clip` doesn't stop the viewport
  scrolling sideways, so clip wide decorations on their section (`.lp-hero`) and check
  `scrollWidth == clientWidth` at 390 px; the viewport is `viewport-fit=cover` (Chrome
  on Android draws edge to edge) and footers pad `env(safe-area-inset-bottom)`; the 3D
  world starts phones at dpr 2 + MSAA 2 and stops adapting when a drop doesn't speed
  frames up (a 30 fps cap, not the GPU).
  **Showcase performance** (2026-10-23, measured in the user's Chrome on an Intel UHD laptop,
  1278x918 at dpr 1.25, 165 Hz, with a benchmark page that scrolls the page per variant):
  the showcase is the 3D, so it never falls back to stills (only without WebGL or if the
  module fails); a software renderer starts at the lowest level, reduced motion cuts.
  What it cost and what replaced it:
  - Phone screens were 2D canvases re-uploaded (780x1688, mipmapped) on every change:
    80-100 ms hitches all through each transition. Now `screen-shader.js` composites the
    app renders (uploaded once) and draws pushes, sheet, banners, taps, spinner and coins.
  - Live VSM shadows cost ~12 ms a frame: now baked at load (`bakeShadows`, a top-down
    lit render with shadows divided by one without; `ShadowMaterial` as an override shaded
    caster tops wrongly). Moving things cast nothing; phones/envelope get painted blobs.
  - Post-processing at the screen's dpr was the last big cost (18 vs 12 ms at dpr 1, where
    it matched no post at all). 8-bit buffers, fewer bloom levels and removing the frosted
    `backdrop-filter` cards changed nothing. Levels keep post down to dpr 1.
  - adapt(): median of ~1 s windows, target 17.5 ms, skips levels that cost the same on
    this screen; a frame cap is only assumed after two drops in a row don't help (one
    unhelpful MSAA drop once pinned the laptop at the top level at 24 fps).
  - Shaders compile (`compileAsync`, incl. the gold key material) and all textures upload
    at load; first use mid-scroll was an 80-100 ms hitch. Scroll scrub is direct (Lenis and
    the camera damping are the only smoothing); cover checks use cached ScrollTriggers.
  Headless Windows Chrome paces WebGL at ~27 fps and pauses covered windows: judge in a
  real window. Frame times quantize to vsync multiples (6.06 ms at 165 Hz).
  **Testing desktop-only CSS** (`(hover: hover) and (pointer: fine)`: the Google Play QR):
  headless Chrome reports neither, so a plain agent-browser run only sees the phone path. Use
  agent-browser >= 0.39 with `--session mouse --args
  --blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4`
  (0.32 split launch args on every comma and Chrome refused to start).
  The whole design/build/verify/ship flow is the project skill `.claude/skills/landing-page`
  (`frames.sh` + `sheet.py` capture a scroll section frame by frame into a contact sheet).
  Chose Astro over Next.js (Vizor's site) to keep a strict CSP with no inline code: the
  landing page bundles its motion (`src/scripts/story.js` → `world.js` Three.js world,
  `ui.js` pointer and text) as same-origin modules; `/join` loads only the hand-written
  `public/assets/join.js` (`<script is:inline src=…>`, never bundled; it's the only code
  that sees the invite). agent-browser reports no fine pointer, so check the custom
  cursor with `/showcase?pointer=fine`; the 3D world needs a few seconds after each scroll
  jump before a screenshot. agent-browser renders WebGL in software, and the page skips
  the world on software renderers (and gives up at runtime if it stays under ~20 fps at
  its lowest quality), so screenshots of the world need `/showcase?world=always`.
  Cache headers live in `public/_headers` (`/assets/...` blocks; `vercel-output.sh`
  turns each into a Vercel route). The **static fallback** (no GPU, reduced motion, no
  JS) shows stills of the world rendered by `infra/site/stills.sh` (capture mode
  `/showcase?still`; also writes the social image `og-mainnet.png`); re-run it after changing the world
  or the story. `public/assets/boot.js` adds `html.js` before the first paint so the
  world's visitors never fetch the stills; anything only for the fallback goes under
  `:is(html:not(.js), html.stills)`.
  **SEO** (`docs/seo.md`): the public origin is one value, `site` in `astro.config.mjs`
  (`https://zafe.cash` since 2026-10-01: Cloudflare DNS, DNS-only CNAMEs to Vercel, www
  redirects to the apex; mail to any `@zafe.cash` address is forwarded by Cloudflare
  Email Routing); canonical, og:url/og:image, `sitemap.xml`, `robots.txt` and
  `.well-known/security.txt` (Astro endpoints in `src/pages/`; security.txt's `Expires`
  is build + 1 year, so redeploy at least yearly) derive from it. `build.sh` validates
  only the two JSON files in `.well-known`. Registered in Google Search Console and Bing
  Webmaster Tools (DNS TXT); each production deploy submits the sitemap's URLs to
  IndexNow (`indexnow.sh`, key file `public/<key>.txt`, public by design; failures only
  warn).
  Head tags live in `Base.astro` (props `title`, `description`, `socialTitle`, `jsonLd`,
  `noindex`); social image `/assets/og-mainnet.png` (1200×630) in `config.ts`. Chat apps and X cache a link's card image by its URL, so when the picture changes, give the file a new name (it was `og.png`, which kept showing the testnet card). `/join` stays
  `noindex`, out of the sitemap and **not** disallowed in robots.txt (Google must fetch
  it to see the noindex). JSON-LD on `/` (Organization, WebSite, MobileApplication,
  FAQPage from the same `faq` array as the page) is the one inline `<script>`
  `build.sh` allows: exactly `type="application/ld+json"`, no other attribute, contents
  must parse as JSON. Raster icons + manifest: `icons.sh` (CSP has `manifest-src 'self'`).
  `astro.config.mjs` sets `inlineStylesheets: 'never'` and `build.format: 'file'`;
  `build.sh` greps the output and fails on any inline script/style/handler. Phone shots
  are the app's own renders (`tool/screens/*_render_test.dart` → `infra/site/assets.py`
  → WebP); fonts are the app's, subset to WOFF2. Calls to action go through `getUrl`/`getLabel`/`getNote` in
  `src/config.ts`: "Get Zafe" only when the build sets `ZAFE_DOWNLOAD_URL`, otherwise
  "View code" to the repo (no APK testers can use before the relay is hosted). Landing copy must only claim shipped
  features. Look at it with agent-browser at 1280 and 390 wide, light and dark.
  **Design** follows `docs/site.md` (now laid out after vizor.cash; decisions at the top): teal only
  for the action and "needs you", gold only for amounts, Space Grotesk only at display
  sizes, real app crops and illustrations (`assets.py` CROPS; wide pillar art is padded to 5:4). The
  chain-view numbers are a real testnet transaction: never replace them with invented
  ones. Patina icons are inlined by `src/components/Icon.astro` (magenta → 38% layer).
  **Deploy**: `.github/workflows/site.yml` → Vercel with `vercel deploy --prebuilt` on
  `.vercel/output` from `vercel-output.sh` (Build Output API v3; headers read from
  `public/_headers`, `overrides` serve `join.html` at `/join`). `builds.json` isn't
  needed (the CLI only reads it for `vercel build` errors). Vercel never builds.
- **CI disk**: the Rust job's test binaries (arti, Halo 2, SQLCipher linked into each)
  filled the runner disk and `ld` died with `signal 7 [Bus error]`. `ci.yml` frees ~30 GB
  first and builds with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`. A bus error or "No
  space left" in a link step means disk, not code.
- **Scanner**: `mobile_scanner` 7.4.2 with our own controller, so `scan_invite_screen.dart`
  handles lifecycle itself (stop on inactive only while running, because the permission
  prompt makes the app inactive; `start()` on resume also picks up a permission granted
  in Settings). `permission_handler` is only used for `openAppSettings()`.
  It bundles ML Kit's barcode model on Android (adds a few MB to the APK).
- `flutter analyze` in a fresh worktree reports errors in
  `rust_builder/cargokit/build_tool` until `dart pub get` runs there (not our code).
- Font family names in code must match `pubspec.yaml` exactly (`Space Grotesk`, `DM Sans`,
  `JetBrains Mono`).
- `pubspec.yaml` must have a single `flutter:` key (a duplicate silently breaks FRB codegen).
- Cargokit is patched (`rust_builder/cargokit/gradle/plugin.gradle`): debug builds no longer
  add x86/x64 unless `-Pzafe.debugEmulatorAbis=true`. Build for a phone with
  `flutter build apk --debug --target-platform android-arm64` (~5 min cold).
  Stale emulator `.so` files can linger in `build/rust_lib_zafe/jniLibs`; delete them.

Device testing (agent-device): emulator AVD `zafe` (API 35 x86_64; needs `/dev/kvm`
access and `libxkbfile.so.1`, which was unpacked into `~/android/sdk/emulator/lib64` without
root). Start it windowed with `emulator -avd zafe -gpu swangle_indirect -no-snapshot
-no-audio`; build with `--target-platform android-x64`. `scripts/app-harness.sh` runs the
relay plus CLI members B and C and sets `adb reverse` for 8787/9067, so the app's localhost
defaults work on a device. Its `cli` subcommand does not rebuild: `cargo build -p zafe-cli`
after core changes. **Emulator renderer (WSL)**: `-gpu swiftshader_indirect` segfaulted in the host
`RenderThread` three times on 2026-09-30 (`dmesg`: "RenderThread: potentially unexpected fatal
signal 11"), around the proposal/sending screens; the emulator died, not the app. Use
`-gpu swangle_indirect` (stable since). AVD RAM raised to 4G for proving. A send's broadcast
can still complete when the emulator dies mid-send: check the vault log (`zafe proposals`)
before retrying. agent-device tips: prefer `find "<text>" click`; refs go stale after
every snapshot; `scroll down --until 'label="..."'` before pressing bottom buttons.

In a fresh git worktree, plain `flutter analyze` reports ~50 errors in
`rust_builder/cargokit/build_tool` until `dart pub get` runs there; `flutter analyze lib
test` checks the app alone.

**Release APKs** (`docs/releasing.md`, `.github/workflows/release.yml`): always
`--split-per-abi` with one `--target-platform`. Plugins (ML Kit) bundle
armeabi-v7a/x86_64 libs our Rust library lacks, so an unsplit APK installs on those
devices and crashes (`libflutter.so is for EM_AARCH64`), and Flutter's Gradle plugin
clears any `buildTypes.*.ndk.abiFilters` you set. Signing reads `android/key.properties`
(gitignored); without it release builds use the debug key.

App commands (from `app/`, after `source ~/android/env.sh`):
`flutter_rust_bridge_codegen generate` (after changing `app/rust/src/api`), `flutter analyze`
(must be clean), `flutter build apk --debug --target-platform android-arm64`,
`adb install -r build/app/outputs/flutter-apk/app-debug.apk`.

Toolchain (installed by `~/android/install-toolchain.sh`; `source ~/android/env.sh`): Flutter at `~/flutter`, JDK 17 at
`~/android/jdk-17`, Android SDK at `~/android/sdk` (platform 36, build-tools 36.0.0).

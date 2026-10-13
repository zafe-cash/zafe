# Vizor wallet reference (for building Zafe)

Upstream: **`github.com/chainapsis/vizor-wallet`** (the Keplr team; Apache-2.0). Studied at **`4bff2e7c64ce8f87404f3e0c5b5d80db1201a5e5`** ("Merge pull request #601 from chainapsis/roman/ironwood-memo-pir", `main`, 2026-09-29). Reviewed 2026-09-29.

History: the first version of this document was written from `ff02152` (2026-07-02, "Merge pull request #302 ... mnemonic-clipboard-expiration") via a stale `github.com/valargroup/vizor-wallet` mirror; Zafe's copied `app/lib/src/core` files come from that snapshot (see section 11, "Resync notes"). Between the two commits upstream merged about 300 PRs (#303–#782): the Ironwood pool migration, Tor on desktop and mobile, Ledger (USB + Bluetooth), Gift Cards (payment links), ZIP-321 payment requests and cross-chain Pay, coinholder voting on mobile, account personalisation, viewing-key export, a custom block explorer, private (PIR) transaction-detail recovery, and mobile keep-awake during sync. The design tokens changed only additively; several shared widgets gained options or changed behavior (section 11). All paths below are relative to the upstream repo root unless stated otherwise.

Vizor's `AGENTS.md` covers architecture and sync. This document covers what it leaves out: exact design tokens, components, mobile screen layouts, the Keystone/PCZT UX, Dart state patterns, flutter_rust_bridge wiring, copy rules and licensing. Everything was read from source. Where the design screenshots disagree with the code, the code is treated as authoritative.

Contents:
1. Design tokens
2. Fonts and assets
3. Component library
4. Mobile shell, navigation and screens
5. Hardware / external-signer UX (Keystone, Ledger)
6. State and data flow in Dart
7. flutter_rust_bridge setup
8. Copy conventions
9. What to copy vs. adapt for Zafe
10. Licensing
11. Resync notes (Zafe's copied files vs. upstream)
12. New upstream features since `ff02152`

---

## 1. Design tokens

All tokens live in `lib/src/core/theme/`. The barrel file `app_theme.dart` exports everything. Widgets read colors with `context.colors.<group>.<token>` and read typography, spacing and radii as static consts (`AppTypography.bodyMedium`, `AppSpacing.sm`, `AppRadii.large`).

### 1.1 Architecture of the token system

| Layer | File | Notes |
|---|---|---|
| Primitives | `theme/primitives.dart` | Raw ladders: `Primitives` (neutral 0–900), `CrimsonPrimitives`, `PlumPrimitives`, `GoldPrimitives`, `GreenPrimitives`. Each step has `pNNNDark` and `pNNNLight` faces, plus explicit alpha variants (`p300Alpha20Dark`, and so on). The comment says "Widgets must never reference these directly." |
| Semantic groups | `theme/colors/app_*_colors.dart` | 12 classes, each with `static const dark` and `static const light`. |
| Aggregate | `colors/app_colors.dart` | `AppColors{background, surface, border, text, icon, button, state, fade, navPanel, shadows, sync, macosUtility}` with `AppColors.dark` / `AppColors.light`. |
| Theme data | `app_theme_data.dart` | `AppThemeData{colors}` with `.dark` / `.light` consts. |
| Inherited widget | `app_theme.dart` | `AppTheme extends InheritedWidget`. `AppTheme.of(context)`. Extension `AppThemeX on BuildContext { appTheme, colors }`. |
| Host | `app_theme_host.dart` | `AppThemeHost(themeMode, child)` resolves `ThemeMode` against platform brightness into `AppTheme(data: AppThemeData.dark|light)`. It also syncs native chrome (see below). |
| Legacy | `legacy_material_theme.dart` | A Material 3 `ThemeData` (Inter body font) passed to `MaterialApp.theme/darkTheme` for stray Material widgets only. New UI never reads it. Since `ff02152` it sets an iOS-only `dialogTheme` with `RoundedSuperellipseBorder(radius AppRadii.xLarge)`. |

Colors are the same on both form factors. Typography and component sizing differ between Desktop and Mobile (1.4).

`AppThemeHost` side effects:
- **Android:** `SystemChrome.setSystemUIOverlayStyle`. Status and navigation bars are painted `background.window`, with icon brightness inverted from the theme and `systemStatusBarContrastEnforced: false` / `systemNavigationBarContrastEnforced: false`. On Android 15+ edge-to-edge, only the icon brightness takes effect.
- **iOS and macOS:** `MethodChannel('com.zcash.wallet/window_appearance').invokeMethod('setBrightness', {'brightness': 'system'|'dark'|'light'})`.

All native channels are still named `com.zcash.wallet/<name>`. At `4bff2e7` the Dart side uses: `background_migration`, `biometric_unlock`, `camera_permission`, `caps_lock`, `date_picker`, `device_owner_auth`, `haptics`, `modal_corners`, `network_privacy`, `numeric_keyboard`, `privacy_shield`, `sensitive_clipboard`, `window_appearance`, `windows_update`.

### 1.2 Primitive ladders (hex)

In the neutral ladder, the light-mode faces run in the reverse direction to the dark ones.

| Step | Dark | Light | Role hint |
|---|---|---|---|
| p0 | #141818 | #FFFFFF | darkest anchor / white |
| p50 | #1B1F1F | #F7F7F7 | base surface |
| p100 | #232828 | #EBEBEB | raised |
| p150 | #2D3232 | #E1E1E1 | overlay |
| p200 | #393E3E | #D4D4D4 | subtle border |
| p300 | #4D5252 | #B8B8B8 | border / disabled icon |
| p400 | #626767 | #9A9A9A | strong border / disabled text |
| p500 | #858686 | #858686 | mid grey (same in both modes) |
| p600 | #A3A4A4 | #626767 | secondary text |
| p700 | #C2C3C3 | #4D5252 | primary text |
| p800 | #F7F7F7 | #2E3232 | accent / primary button fill |
| p900 | #FFFFFF | #141818 | max contrast |

Brand and utility ladders. Each shows the Dark face; the Light face is the same list mirrored.

| Ladder | p0 | p50 | p100 | p150 | p200 | p300 | p400 | p500 | p600 | p700 | p800 | p900 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Crimson (brand) | #080305 | #19080F | #32111D | #4C192C | #6D243F | #862D4E | #A83861 | #B35074 | #BE6A88 | #CF92A8 | #E1B9C8 | #F5EBEE |
| Plum (destructive) | #0B060D | #2F133A | #4E205F | #5E2673 | #772E89 | #9338A7 | #AB40BF | #B85BC8 | #C06ECE | #CD8CD9 | #E6C5EC | #F6ECF9 |
| Gold (warning/"success") | #0E0905 | #180F08 | #241810 | #36251A | #4D3624 | #6E4E33 | #956B45 | #CD9F64 | #DDB37A | #EAC890 | #F5E0B8 | #FCF8F2 |
| Green (positive) | #001E0A | #023A21 | #005B35 | #007F49 | #00A460 | #0DC87D | #3BD38B | #64DD9C | #89E5B0 | #A9ECC4 | #C2F5D5 | #D3FFE4 |

The key brand color is **crimson #A83861**: `CrimsonPrimitives.p400Dark` is the same value as `p300Light`. Destructive is **plum**, not red. The "success" text token is **gold**. Positive amounts use **green** (`text.positiveStrong`).

### 1.3 Semantic color tokens (resolved to hex)

The table below was generated by resolving every token in `colors/*.dart` against the primitives. `@N%` means alpha.

Changes since `ff02152` (all in this section, marked "new" in the tables): `background.utilityDestructiveStrong` (fill of the destructive toast), `border.utilityPositiveStrong`, the primary button's dark `border` (now white @ 10%), and **`surface.input` became a group**: `AppSurfaceColors.input` is now an `AppInputSurfaceColors{primary, secondary, focus}` (same file), replacing the flat `input` / `inputFocus` colors. That last one is a breaking rename: callers now read `colors.surface.input.primary` (e.g. `MobileTextField`). Primitives, typography, spacing, radii, sizing, shadows and the other eight color files (fade, icon, macosUtility, navPanel, shadow, state, sync, text) are byte-identical to `ff02152`.


#### AppBackgroundColors

| Token | Dark | Light |
|---|---|---|
| `window` | #0F0F0F | #F7F7F7 |
| `ground` | #1B1F1F (p50Dark) | #FFFFFF (p0Light) |
| `base` | #232828 (p100Dark) | #F7F7F7 (p50Light) |
| `raised` | #2D3232 (p150Dark) | #EBEBEB (p100Light) |
| `overlay` | #393E3E (p200Dark) | #E1E1E1 (p150Light) |
| `inverse` | #F7F7F7 (p800Dark) | #2E3232 (p800Light) |
| `neutralScrim` | #141818 @50% (p0Alpha50Dark) | #141818 @50% (p900Alpha50Light) |
| `neutralSubtleOpacity` | #626767 @20% (p400Alpha20Dark) | #B8B8B8 @20% (p300Alpha20Light) |
| `neutralStrongOpacity` | #4D5252 @50% (p300Alpha50Dark) | #B8B8B8 @35% (p300Alpha35Light) |
| `brandCrimsonSubtle` | #32111D (crimson.p100Dark) | #F6EBEF (crimson.p0Light) |
| `brandCrimsonStrong` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |
| `brandCrimsonAlpha` | #862D4E @35% (crimson.p300Alpha35Dark) | #A83861 @15% (crimson.p300Alpha15Light) |
| `utilityDestructiveSubtle` | #2F133A (plum.p50Dark) | #F6ECF9 (plum.p0Light) |
| `utilityDestructiveStrong` (new) | #9338A7 (plum.p300Dark) | #9338A7 (plum.p400Light) |
| `utilityDestructiveAlphaSubtle` | #AB40BF @8% (plum.p400Alpha8Dark) | #9338A7 @8% (plum.p400Alpha8Light) |
| `utilityDestructiveAlpha` | #AB40BF @25% (plum.p400Alpha25Dark) | #9338A7 @15% (plum.p400Alpha15Light) |
| `utilitySuccessSubtle` | #36251A (gold.p150Dark) | #F8EDDA (gold.p50Light) |
| `utilitySuccessStrong` | #CD9F64 (gold.p500Dark) | #CD9F64 (gold.p300Light) |
| `utilitySuccessAlpha` | #0DC87D @15% (green.p300Alpha15Dark) | #3BD38B @15% (green.p300Alpha15Light) |
| `homeCard` | #1B1F1F (p50Dark) | #2E3232 (p800Light) |

#### AppBorderColors

| Token | Dark | Light |
|---|---|---|
| `subtle` | #2D3232 (p150Dark) | #E1E1E1 (p150Light) |
| `subtleOpacity` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `inverseOpacity` | #2D3232 @15% (p150Alpha15Dark) | #FFFFFF @10% (p0Alpha10Light) |
| `regular` | #4D5252 (p300Dark) | #D4D4D4 (p200Light) |
| `medium` | #626767 (p400Dark) | #B8B8B8 (p300Light) |
| `strong` | #F7F7F7 (p800Dark) | #141818 (p900Light) |
| `utilityDestructive` | #AB40BF (plum.p400Dark) | #AB40BF (plum.p300Light) |
| `utilityDestructiveSubtle` | #4E205F (plum.p100Dark) | #CD8CD9 (plum.p100Light) |
| `utilitySuccess` | #CD9F64 (gold.p500Dark) | #B0844F (gold.p400Light) |
| `utilityPositiveStrong` (new) | #0DC87D (green.p300Dark) | #00A460 (green.p500Light) |
| `brandCrimsonStrong` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |

#### AppPrimaryButtonColors

| Token | Dark | Light |
|---|---|---|
| `bg` | #F7F7F7 (p800Dark) | #2E3232 (p800Light) |
| `bgHover` | #862D4E (crimson.p300Dark) | #862D4E (crimson.p400Light) |
| `bgPressed` | #862D4E (crimson.p300Dark) | #862D4E (crimson.p400Light) |
| `border` | #FFFFFF @10% (p900Alpha10Dark; was #2D3232 @15% p150Alpha15Dark at `ff02152`) | #FFFFFF @10% (p0Alpha10Light) |
| `borderHover` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `borderPressed` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `label` | #1B1F1F (p50Dark) | #EBEBEB (p100Light) |
| `labelHover` | #F7F7F7 (p800Dark) | #EBEBEB (p100Light) |

#### AppSecondaryButtonColors

| Token | Dark | Light |
|---|---|---|
| `bg` | #2D3232 (p150Dark) | #FFFFFF (p0Light) |
| `bgHover` | #393E3E (p200Dark) | #EBEBEB (p100Light) |
| `bgPressed` | #393E3E (p200Dark) | #EBEBEB (p100Light) |
| `label` | #F7F7F7 (p800Dark) | #141818 (p900Light) |

#### AppGhostButtonColors

| Token | Dark | Light |
|---|---|---|
| `bg` | #141818 (p0Dark) | #FFFFFF (p0Light) |
| `bgHover` | #2D3232 (p150Dark) | #EBEBEB (p100Light) |
| `border` | #4D5252 (p300Dark) | #B8B8B8 (p300Light) |
| `label` | #C2C3C3 (p700Dark) | #2E3232 (p800Light) |

#### AppDisabledButtonColors

| Token | Dark | Light |
|---|---|---|
| `bg` | #4D5252 @20% (p300Alpha20Dark) | #B8B8B8 @20% (p300Alpha20Light) |
| `label` | #858686 @50% (p500Alpha50Dark) | #858686 @50% (p500Alpha50Light) |

#### AppDestructiveButtonColors

| Token | Dark | Light |
|---|---|---|
| `bg` | #772E89 (plum.p200Dark) | #772E89 (plum.p500Light) |
| `bgHover` | #5E2673 (plum.p150Dark) | #5E2673 (plum.p600Light) |
| `bgPressed` | #5E2673 (plum.p150Dark) | #5E2673 (plum.p600Light) |
| `border` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `borderHover` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `borderPressed` | #FFFFFF @10% (p900Alpha10Dark) | #141818 @5% (p900Alpha5Light) |
| `label` | #E6C5EC (plum.p800Dark) | #E6C5EC (plum.p50Light) |

#### AppFadeColors

| Token | Dark | Light |
|---|---|---|
| `illustration` | #141818 @50% (p0Alpha50Dark) | #141818 @0% (p0Alpha0Dark) |

#### AppIconColors

| Token | Dark | Light |
|---|---|---|
| `accent` | #F7F7F7 (p800Dark) | #141818 (p900Light) |
| `regular` | #C2C3C3 (p700Dark) | #4D5252 (p700Light) |
| `muted` | #858686 (p500Dark) | #858686 (p500Light) |
| `disabled` | #4D5252 (p300Dark) | #B8B8B8 (p300Light) |
| `inverse` | #141818 (p0Dark) | #FFFFFF (p0Light) |
| `onPrimary` | #141818 (p0Dark) | #FFFFFF (p0Light) |
| `warning` | #CD9F64 (gold.p500Dark) | #CD9F64 (gold.p300Light) |
| `destructive` | #AB40BF (plum.p400Dark) | #AB40BF (plum.p300Light) |
| `destructiveLight` | #9338A7 (plum.p300Dark) | #B85BC8 (plum.p200Light) |
| `success` | #0DC87D (green.p300Dark) | #00A460 (green.p500Light) |
| `brandCrimson` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |

#### AppMacosUtilityColors

| Token | Dark | Light |
|---|---|---|
| `window` | #0F0F0F | #F7F7F7 |
| `windowTransparent` | #0F0F0F @0% | #F5F5F5 @0% |
| `navPanel` | #1A1A1A @30% | #FFFFFF @30% |
| `font` | #FFFFFF @80% | #1A1A1A @85% |
| `thinBorder` | #1A1A1A @23% | #FFFFFF @55% |
| `innerBorder` | #FFFFFF @15% | #FFFFFF @15% |

#### AppNavPanelColors

| Token | Dark | Light |
|---|---|---|
| `badgeBg` | #862D4E (crimson.p300Dark) | #A83861 (crimson.p300Light) |
| `activeBg` | #A83861 @15% (crimson.p400Alpha15Dark) | #A83861 @10% (crimson.p300Alpha10Light) |
| `activeIcon` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |
| `activeLabel` | #F5EBEE (crimson.p900Dark) | #19080F (crimson.p800Light) |

#### AppShadowColors

| Token | Dark | Light |
|---|---|---|
| `shadow1` | #141818 @0% (p0Alpha0Dark) | #E1E1E1 (p150Light) |
| `shadow2` | #141818 @0% (p0Alpha0Dark) | #B8B8B8 (p300Light) |
| `shadow3` | #141818 @0% (p0Alpha0Dark) | #141818 @20% (p900Alpha20Light) |
| `subtle` | #141818 @0% (p0Alpha0Dark) | #141818 @5% (p900Alpha5Light) |
| `regular` | #141818 @0% (p0Alpha0Dark) | #141818 @10% (p900Alpha10Light) |

#### AppStateColors

| Token | Dark | Light |
|---|---|---|
| `hover` | #232828 (p100Dark) | #F7F7F7 (p50Light) |
| `hoverOpacity` | #141818 @15% (p0Alpha15Dark) | #141818 @5% (p900Alpha5Light) |
| `pressed` | #2D3232 (p150Dark) | #E1E1E1 (p150Light) |
| `focus` | #393E3E (p200Dark) | #D4D4D4 (p200Light) |
| `selected` | #2D3232 (p150Dark) | #E1E1E1 (p150Light) |
| `selectedOpacity` | #141818 @30% (p0Alpha30Dark) | #141818 @5% (p900Alpha5Light) |
| `focusRing` | #F7F7F7 (p800Dark) | #141818 (p900Light) |
| `focusGap` | #141818 (p0Dark) | #FFFFFF (p0Light) |
| `focusRingBrand` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |
| `focusRingDestructive` | #772E89 (plum.p200Dark) | #9338A7 (plum.p400Light) |

#### AppSurfaceColors

| Token | Dark | Light |
|---|---|---|
| `card` | #232828 (p100Dark) | #F7F7F7 (p50Light) |
| `input.primary` | #1B1F1F (p50Dark) | #FFFFFF (p0Light) |
| `input.secondary` (new) | #2D3232 (p150Dark) | #EBEBEB (p100Light) |
| `input.focus` | #232828 (p100Dark) | #F7F7F7 (p50Light) |
| `nav` | #1B1F1F (p50Dark) | #FFFFFF (p0Light) |
| `navActive` | #2D3232 (p150Dark) | #EBEBEB (p100Light) |
| `tooltip` | #393E3E (p200Dark) | #2E3232 (p800Light) |
| `qrCode` | #FFFFFF (p0Light) | #FFFFFF (p0Light) |
| `scrollbarThumb` | #393E3E (p200Dark) | #E1E1E1 (p150Light) |

#### AppSyncColors

| Token | Dark | Light |
|---|---|---|
| `text` | #D3FFE4 (green.p900Dark) | #005B35 (green.p700Light) |
| `textSyncing` | #D3FFE4 @65% (green.p900Alpha65Dark) | #001E0A @65% (green.p900Alpha65Light) |
| `textError` | #FFFFFF @50% (p900Alpha50Dark) | #141818 @50% (p900Alpha50Light) |
| `glow` | #858686 (p500Dark) | #64DD9C (green.p200Light) |
| `lightSuccess` | #0DC87D (green.p300Dark) | #0DC87D (green.p400Light) |
| `lightError` | #A3A4A4 (p600Dark) | #858686 (p500Light) |

#### AppTextColors

| Token | Dark | Light |
|---|---|---|
| `accent` | #FFFFFF (p900Dark) | #141818 (p900Light) |
| `primary` | #C2C3C3 (p700Dark) | #4D5252 (p700Light) |
| `secondary` | #A3A4A4 (p600Dark) | #626767 (p600Light) |
| `muted` | #858686 (p500Dark) | #858686 (p500Light) |
| `disabled` | #626767 (p400Dark) | #9A9A9A (p400Light) |
| `inverse` | #141818 (p0Dark) | #FFFFFF (p0Light) |
| `warning` | #CD9F64 (gold.p500Dark) | #B0844F (gold.p400Light) |
| `positiveStrong` | #3BD38B (green.p400Dark) | #00A460 (green.p500Light) |
| `destructive` | #B85BC8 (plum.p500Dark) | #AB40BF (plum.p300Light) |
| `destructiveLight` | #AB40BF (plum.p400Dark) | #C06ECE (plum.p150Light) |
| `success` | #CD9F64 (gold.p500Dark) | #B0844F (gold.p400Light) |
| `brandCrimson` | #A83861 (crimson.p400Dark) | #A83861 (crimson.p300Light) |
| `homeCard` | #F7F7F7 (p800Dark) | #FFFFFF (p0Light) |

**Semantic roles worth internalizing:**
- **Surfaces.** Screen background is `background.window` (#0F0F0F dark / #F7F7F7 light). Cards are `background.ground` (#1B1F1F / white). Sheets are `background.base`. Segmented-control tracks and unselected option fills are `background.raised`. Progress tracks are `background.overlay`.
- **Primary button.** The primary button is an inverse neutral pill (near-white in dark mode, #2E3232 in light) whose hover/pressed fill turns **crimson #862D4E**. Crimson is used sparingly: shield icons, focus rings, active tab tint, passcode dots, address-chunk highlights.
- **Home card.** `background.homeCard` is a dark card in *both* themes (#1B1F1F dark / #2E3232 light) with `text.homeCard` light text. It is used for the balance card, seed card, onboarding info cards and the "Create wallet" method card.
- **Shadows.** All `shadows.*` are alpha 0 in dark mode, so raised surfaces lose their shadow automatically.
- **Scrim.** `background.neutralScrim` = #141818 @ 50% in both modes. It is the barrier color for all sheets.
- **Icon disc.** `background.neutralSubtleOpacity` is the standard fill for 40px icon discs, passcode keys and empty radios.

### 1.4 Typography

The Figma `Fonts` collection has Desktop and Mobile modes. Both are materialized as full const sets, `AppTypographyDesktop` and `AppTypographyMobile`, in `theme/app_typography.dart`. `AppTypography.*` picks one at compile time. Line heights are stored as `height = figmaLineHeightPx / fontSize`. Colors are never baked in: callers use `.copyWith(color: ...)`.

| Token (`AppTypography.`) | Figma name | Family / weight | Mobile size / line / tracking | Desktop size / line / tracking |
|---|---|---|---|---|
| `displayLarge` (= `displayMedium`) | Headline XL | Young Serif 400 | 40 / 40 / −1.35 | 45 / 48 / −1.35 |
| `displaySmall` = `headlineLarge` | Headline L | Young Serif 400 | 32 / 33 / 0 | 32 / 33 / 0 |
| `headlineMedium` | Headline M | Young Serif 400 | 28 / 30 / −0.28 | 28 / 30 / −0.28 |
| `headlineSmall` | Headline S | Geist 500 | 18 / 22 / 0 | 16 / 20 / 0 |
| `bodyLarge` | Body L | Geist 500 | 18 / 26 / −0.24 | 16 / 24 / −0.24 |
| `bodyMedium` | Body M | Geist 400 | 16 / 25 / −0.21 | 14 / 21 / −0.21 |
| `bodyMediumStrong` | Body M Medium | Geist 500 | 16 / 25 / −0.21 | 14 / 21 / −0.21 |
| `bodySmall` | Body S | Geist 400 | 14 / 20 / −0.12 | 12 / 18 / −0.12 |
| `bodyExtraSmall` | Body XS | Geist 400 | 13 / 18 / −0.055 | 11 / 16 / −0.055 |
| `labelLarge` | Label M | Geist 500 | 16 / 17 / −0.06 | 14 / 16 / −0.06 |
| `labelMedium` = `labelSmall` | Label S | Geist 500 | 14 / 15 / 0 | 13 / 14 / 0 |
| `codeMedium` | Code M | Geist Mono 500 | 16 / 21 / 0 | 14 / 21 / 0 |
| `codeSmall` | Code S | Geist Mono 500 | 13 / 17 / 0 | 13 / 17 / 0 |

Young Serif font features:
- Mobile serif styles set `fontFeatures: [FontFeature.liningFigures()]`.
- Desktop serif styles set `FontFeature.enable('case')`.
- Both exist because Young Serif defaults to old-style figures, which look wrong in balances.
- `appSerifDisplayStyle({required Color color})` in `app_serif_display.dart` returns `displaySmall.copyWith(color:)`.

Typography used directly on screens without a token:
- Home balance: Young Serif 45/48, −1.35, with a Young Serif 32 "ZEC" suffix.
- Send amount: `displayLarge` at fontSize 48, w500, line height 40px.
- Welcome hero: Young Serif 48, height 1.1, −1.35.
- Sheet titles: `bodyLarge` with `fontWeight: w600`.
- Account name in the top nav: `labelLarge` w600.

### 1.5 Spacing, radii, icon sizes (mode-invariant)

`AppSpacing` (`app_spacing.dart`). Note that `base` is 32; it is not the middle of the scale.

| xxs | xs | s | sm | md | base | lg | xl | xl2 | xl3 |
|---|---|---|---|---|---|---|---|---|---|
| 4 | 8 | 12 | 16 | 24 | 32 | 48 | 64 | 96 | 128 |

`AppRadii` (`app_radii.dart`): `xSmall 8`, `small 12`, `medium 16`, `large 24`, `xLarge 32`, `full 999` (prefer `StadiumBorder` for pills).
- Figma radius names are shifted one step against Dart: Figma SM/M/L = Dart medium/large/xLarge. Map by value, not by name.
- Common literal radii: `MobileSurfaceCard.defaultRadius = 20` (most screens override it to 24), 28 on some Keystone permission cards, and `AppWindowSizing.paneRadius = 20` (desktop).

`AppIconSize`: `medium 16`, `large 24`. Other icon sizes used in practice: 20 inside buttons and inputs on mobile, 28 in the tab bar, 18 inside 40px discs.

### 1.6 Component sizing (form-factor dependent, `app_sizing.dart`)

| Class | Field | Desktop | Mobile |
|---|---|---|---|
| `AppAssetSize` | size / icon / padding | 32 / 16 / 4 | 40 / 18 / 0 |
| `AppButtonSizing` | largeHeight | 44 | **50** |
| | mediumSmallIconSize | 16 | 20 |
| `AppInputSizing` | height | 46 | **60** |
| | iconWrapWidth / iconSize | 32 / 20 | 36 / 24 |
| | radius | 12 (`small`) | 16 (`medium`) |
| `AppWindowSizing` (desktop only) | min 1080×720, max 1296×864, content max width 420, pane radius 20 | | |

`AppProfilePictureSize`: medium 24, large 32, navLarge 40, xLarge 56, xxLarge 72.

Fixed chrome constants:

| Constant | Value |
|---|---|
| `kMobileTopNavHeight` | 72 |
| `kMobileTabBarHeight` | 64 |
| `MobileListRow.minHeight` | 52 (settings uses 44) |
| `kIosHomeIndicatorClearance` | 16 |

### 1.7 Shadows

`appSurfaceShadow(AppColors c)` (`app_shadows.dart`) has four layers, all using `c.shadows.subtle` (#141818 @ 5% light, transparent dark):

```dart
[BoxShadow(color: s, blurRadius: 1),
 BoxShadow(color: s, offset: Offset(0,1), blurRadius: 2),
 BoxShadow(color: s, offset: Offset(0,2), blurRadius: 4),
 BoxShadow(color: s, blurRadius: 1)]
```

This is used by text fields, `ReviewWrapCard` and the home balance card.

The modal card shadow (`_modalShadow` in `app_mobile_sheet.dart`, and the same in `AppModalCard`) is hard-coded black and theme-independent:

```dart
[BoxShadow(color: Color(0x14000000), offset: Offset(0,14), blurRadius: 28),
 BoxShadow(color: Color(0x08000000), offset: Offset(0,-6), blurRadius: 12),
 BoxShadow(color: Color(0x0F000000), offset: Offset(0,2),  blurRadius: 8)]
```

It is combined with an inner rim, painted by `_ModalInnerHighlightPainter`: a 1px stroke of `0x26FFFFFF` with `MaskFilter.blur(BlurStyle.inner, 2)`.

Tab bar shadow: a 1px spread ring in `macosUtility.thinBorder`, plus `BoxShadow(black @ 5%, offset (0,25), blur 25)`.

### 1.8 Motion

| Where | Duration / curve |
|---|---|
| Onboarding route transitions (`core/motion/onboarding_motion.dart`) | forward 320ms `easeOutCubic`; reverse 260ms `easeInCubic` (`kOnboardingForwardDuration`, `kOnboardingReverseDuration`, `kOnboardingForwardCurve`, `kOnboardingReverseCurve`) |
| `AppButton` fill/border change | 120ms `easeOut` (ghost: 0ms); focus ring fades in over 120ms |
| Tab bar selection pill + icon tint | 240ms, `Cubic(0.34, 1.40, 0.64, 1.0)` (easeOutBack with half the overshoot) |
| Tab icon pop on activate | 250ms, scale 1.0 → 1.12 (easeOut, 45%) → 1.0 (easeIn, 55%) |
| Tab icon press | scale 0.9; 90ms press (`easeOut`) / 180ms release (`easeOutBack`) |
| Sync status shimmer + edge-bar glow breathing | period 1400ms; glow blur 8→13, alpha 0.2→0.45, cosine breath |
| Receive segmented control indicator | 160ms `easeOut`; PageView `animateToPage` 220ms |
| Home transparent-balance strip | 280ms in / 240ms out, `easeOutCubic`, size + fade + slide |
| Skeleton shimmers | price bar 1200ms repeat; Keystone QR placeholder 1400ms repeat |
| Animated UR QR | 200ms per frame (5 fps; `_keystoneQrFrameInterval`, was 100ms at `ff02152`) |
| Toast | visible for 2s (`AppToast.defaultDuration`); appears and disappears with no animation |
| Push routes (mobile) | `CupertinoPage`, the native iOS slide with edge-swipe back |
| Tab roots | `NoTransitionPage` |
| iOS sheet corners (new) | `MobileModalCorners` animates the adaptive bottom radius over 250ms |
| `AppCarousel` (new) | autoplay every 5s, 400ms page transition |

Every animated widget checks `MediaQuery.maybeDisableAnimationsOf(context)` and snaps instantly under reduced motion. `AppLoadingIcon` only started doing so after `ff02152`.

### 1.9 Form factor selection (`core/layout/app_form_factor.dart`)

The form factor is a **build-time** constant. There is one binary per form factor, and unused tokens are tree-shaken.

```dart
enum AppFormFactor { desktop, mobile }

const String _formFactorDefine =
    String.fromEnvironment('VIZOR_FORM_FACTOR', defaultValue: 'desktop');

const AppFormFactor kAppFormFactor =
    _formFactorDefine == 'mobile' ? AppFormFactor.mobile : AppFormFactor.desktop;

bool get isDesktopLayoutPlatform =>
    !kIsWeb && (Platform.isMacOS || Platform.isWindows || Platform.isLinux);

// main(): assert(debugCheckFormFactorMatchesPlatform());
// throws StateError('... Rebuild with --dart-define=VIZOR_FORM_FACTOR=mobile.')

// Token selector pattern (app_typography.dart / app_sizing.dart):
const _mobile = kAppFormFactor == AppFormFactor.mobile;
abstract final class AppTypography {
  static const bodyMedium =
      _mobile ? AppTypographyMobile.bodyMedium : AppTypographyDesktop.bodyMedium;
  ...
}
```

Mobile builds need `--dart-define=VIZOR_FORM_FACTOR=mobile`. The router also branches on `kAppFormFactor` (section 6.2).

**For Zafe (mobile-only):** drop the Desktop sets and keep a single `AppTypography` / `AppSizing` with the mobile values. Keeping the `*Mobile` names costs nothing if a tablet or desktop build might come later.

---

## 2. Fonts and assets

### 2.1 Fonts (`assets/fonts/`, declared in `pubspec.yaml` `flutter.fonts`)

| Family | Files (weights) | Used for | License (from the font's embedded name table) |
|---|---|---|---|
| **Geist** | Regular 400, Medium 500, SemiBold 600, Bold 700 (v1.800) | All sans UI text: body, labels, buttons, `headlineSmall` | SIL OFL 1.1. "Copyright 2024 The Geist Project Authors (github.com/vercel/geist-font)" |
| **Geist Mono** | Regular 400, Medium 500 (v1.700) | `codeMedium` / `codeSmall`: mnemonic indices, txids, some address chips | SIL OFL 1.1, same authors |
| **Young Serif** | Regular 400 only (v6.002) | All display/headline serif: balances, amounts, screen titles in the top nav, onboarding titles, passcode digits | SIL OFL 1.1. "Copyright 2023 The Young Serif Project Authors (github.com/noirblancrouge/YoungSerif)" |
| **Inter** | 400/500/600/700 (v4.001) | Only `legacy_material_theme.dart` (the Material fallback theme). Effectively unused by the design system. | Embedded strings reference openfontlicense.org. "Copyright 2016 The Inter Project Authors (github.com/rsms/inter)". Inter is OFL 1.1. |

**The repo ships no `OFL.txt` license files for any of these fonts.** The OFL permits bundling but requires the copyright notice and license to accompany redistributed copies. When Zafe bundles these fonts, it should add `assets/fonts/OFL-Geist.txt`, `OFL-YoungSerif.txt` (and `OFL-Inter.txt` if used), and register them with `LicenseRegistry.addLicense` so they appear in `showLicensePage`. Zafe can also skip Inter entirely.

### 2.2 Icons (`assets/icons/`, 116 files; 86 at `ff02152`)

Icons are monochrome single-fill SVGs on a 24×24 viewBox that keeps the Figma frame padding. `lib/src/core/widgets/app_icon.dart` defines them.

- `abstract final class AppIcons` holds string constants that map to `assets/icons/<name>.svg`. The set: `addNew, arrowBack, arrowBottomLeft, arrowDown, arrowDownCircle, arrowDownward, arrowForwardIos, arrowTopRight, arrowUpward, backspace, block, book, calendar, camera, cameraDenied, cancel, check, checkCircle, chevronBackward, chevronForward, collapsed, cog, coins, copy, cross, crystalBall, day, doubleArrowVertical, dragon, edit, editFilled, endpoint, eye, eyeClosed, expand, faceId, filter, github, globe, help, history, home, importWallet, key, keystone, link, loader, lock, logOut, monitor, night, options, plane, plus, qr, qrCodeFill, renew, scroll, search, share, shieldAsset, shieldKeyhole, shieldKeyholeOutline, skip, skull, swapArrows, sync, theme, time, transparentBalance, unlock, trash, user, users, uturnUp, vizor, wallet, warning, zcash, zcashCurrency`.
- Added since `ff02152` (30 files): `bell`, `coinholder_voting`, `donation`, `gift_card`, `gift_card_outline`, `ironwood_migration_loader`, `keystone_scan`, `keystone_scan_help_pointer`, `ledger`, `ledger_brand`, `migration_fast`, `migration_sign`, `migration_split`, `migration_timer`, `minus`, `money_bag`, `notification_bell`, `paid`, `paste`, `pause`, `payment_link_share_badge.png` (raster), `play`, `sub_arrow`, `tor`, `touch_id`, `vote`, `voting_award_gold`, `voting_award_star`, `warning_circle`, `wrench`. `AppIcons` gained the matching constants (`bell, coinholderVoting, donation, touchId, giftCard, giftCardOutline, keystoneScan, ledger, ledgerBrand, moneyBag, migrationFast, ironwoodMigrationLoader, migrationSign, migrationSplit, migrationTimer, minus, notificationBell, paid, paste, pause, play, subArrow, tor, vote, warningCircle, wrench`). `ledger*` and `tor` are third-party marks.
- `AppIcon(String name, {double size = 16, Color? color, bool animated = true, String? semanticLabel})` renders `SvgPicture.asset('assets/icons/$name.svg', colorFilter: ColorFilter.mode(color, BlendMode.srcIn))`.
- **Tint resolution order:** explicit `color`, then `IconTheme.of(context).color` (so `AppButton` tints its icons automatically), then `colors.icon.regular`.
- `AppIcons.loader` is code-drawn by `AppLoadingIcon` (`app_loading_icon.dart`): eight rounded spokes, with an opacity pulse stepping around them.
- Raster exceptions: `receive_qr_shield_crimson.png` (96×96, the crimson shield QR centre badge), `receive_qr_transparent_{light,dark}.png`, `network_zec.png`.
- The icon art is Vizor/Keplr's own work. It is Apache-2.0 as part of the repo, but distinctive glyphs like `shield_keyhole` and `vizor` are brand-adjacent.

Semantic icon choices to mirror:

| Meaning | Icon |
|---|---|
| Send | `plane` |
| Receive | `arrowDownCircle` |
| Shielded | `shieldKeyhole` (tinted crimson) |
| Transparent | `transparentBalance` (tinted `icon.muted`) |
| Tab: home | `home` |
| Tab: activity | `history` |
| Tab: settings | `cog` |
| Close | `cross` |
| Back | `chevronBackward` |
| Row chevron | `chevronForward` |
| Explorer link | `arrowTopRight` |
| Help | `help` |
| Fee/memo toggle | `doubleArrowVertical` |
| New shielded address | `renew` |

### 2.3 Illustrations and other raster assets

- **`assets/illustrations/`** (97 entries at `4bff2e7`, 64 at `ff02152`; most with `_light` / `_dark` variants, plus a `payment_links/` folder of 14 Gift Card artworks such as `payment_link_card_knight.png`, `payment_link_card_dragon.png`): a hand-drawn **medieval knight** universe (engraving/comic style, crimson accents). Examples:
  - `mobile_welcome_hero.png` (786×1704): a knight lounging on a throne among treasure chests; bottom-anchored, black above.
  - `send_status_illustration_dark.png`: a knight flipping a ZEC coin.
  - Also: `home_rest_character.png`, `biometrics_faceid_knight.png`, `settings_backdrop_castle.png`, `method_*` cards, and the onboarding sidebars.
  - `home_balance_card_pattern_*.png` and `onboarding_card_pattern.png` are subtle texture overlays for the dark cards.
  - New since `ff02152`: `ironwood_migration_*` (intro banners, how-it-works steps, expectation cards, done coins/ribbon, modal background/symbol, home card background, `ironwood_wordmark.svg`), `donation_*`, `method_{create,import,keystone,ledger,link_desktop}_card_bg.png` (method-selection card backgrounds), `onboarding_customise_account_sidebar.png`, `onboarding_ledger_sidebar.png`, `wallet_link_{success,expired}.png`, `keystone_qr_scan_error.png`, `mobile_send_status_background.png`, `mnemonic_invalid_underline.svg`, `ironwood_notification_bell.png`.
- **`assets/profile_pictures/`**: 15 avatar PNGs, named in settings as "Knight", "Viking", "Samurai", "Wizard" and so on.
- **`assets/swap/tokens/`** (109 files) and **`assets/swap/chains/`**: third-party token and chain logos.

**These illustrations are Vizor's brand identity.** Zafe should not reuse them (section 10), and they are the one part of "their UI" Zafe must replace. The layouts reserve explicit art slots:

| Slot | Size |
|---|---|
| Welcome hero | full-bleed, bottom-anchored |
| Home empty state | 340×220 canvas |
| Biometrics hero | 321 tall |
| Method-selection cards | 180×120 art on the right |
| Home importing background | 1080×720 |

Zafe can fill these with its own art or drop them.

---

## 3. Component library

Shared widgets live in `lib/src/core/widgets/` (mobile-specific ones in `widgets/mobile/`) and `lib/src/core/layout/mobile/`. Most are browsable in Widgetbook (`lib/widgetbook/*_use_cases.dart` plus `fixtures/`; run with `widgetbook.sh` or `fvm flutter run -t lib/widgetbook.dart --dart-define=VIZOR_FORM_FACTOR=mobile`). New core directories since `ff02152`: `core/input/` (desktop password input source, caps-lock monitor), `core/lifecycle/` (`SigningShutdownHost`), `core/network/network_http_client.dart` (Tor-aware HTTP, 6.5), `core/sharing/share_png.dart`, `core/zcash/zip321_payment_request_builder.dart`.

### 3.1 Buttons: `AppButton` (`widgets/app_button.dart`)

```dart
AppButton({required VoidCallback? onPressed, required Widget child,
  AppButtonVariant variant = primary,   // primary | secondary | ghost | destructive
  AppButtonSize size = large,           // large | mediumLarge | medium | small
  double? height, EdgeInsets? contentPadding, Widget? leading, Widget? trailing,
  double? minWidth, double? iconGap, Color? focusRingColor,
  Color? disabledBackgroundColor, Color? enabledBorderColor,
  FocusNode? focusNode, bool autofocus = false,
  bool expand = false,            // true = fill parent width (mobile CTAs)
  bool constrainContent = false,  // allow label to ellipsize
  // new since ff02152:
  bool growWithContent = false,   // height becomes a minimum; wrapped labels grow the button
  BorderRadius? borderRadius,     // null = pill; else RoundedRectangleBorder, focus ring follows
  Color? enabledBackgroundColor, pressedBackgroundColor,   // pressed also used for hover
  Color? enabledLabelColor, pressedLabelColor})
```

- **Shape:** `StadiumBorder` pill by default (`borderRadius` overrides it for composed rows). `onPressed == null` means disabled.
- **Accessibility:** upstream still wraps the button only in `MouseRegion` + `GestureDetector` (no `Semantics(button: true)`); Zafe's copy adds `MergeSemantics(Semantics(button, enabled, onTap))` (section 11).
- **Fill priority:** disabled > pressed > hover > default.
- **Keyboard:** Space/Enter activate the button.

Sizes:

| Size | Height | Padding (h / v) | Icon | Gap | Label style |
|---|---|---|---|---|---|
| `large` | `AppButtonSizing.largeHeight` (50 mobile) | 16 / 8 | 20 | 4 | `labelLarge` |
| `mediumLarge` | 36 | 12 / 4 | 16 | 4 | `labelLarge` |
| `medium` | 32 | 8 / 4 | 20 mobile / 16 desktop | 4 | mobile `labelLarge`, desktop `labelMedium` |
| `small` | 24 | 4 / 0 | 20 mobile | 4 | `labelSmall` |

The label also gets an extra 4px of horizontal padding on every size except small.

Variants:

| Variant | bg / hover+pressed | Border | Label |
|---|---|---|---|
| primary | `button.primary.bg` (#F7F7F7 dark / #2E3232 light) → `bgHover` crimson #862D4E | 1.5px `button.primary.border` | `button.primary.label` |
| secondary | `button.secondary.bg` (#2D3232 / white) → `bgHover` | none | `button.secondary.label` |
| ghost | transparent → `button.ghost.bgHover` | none | `button.ghost.label` |
| destructive | plum #772E89 → #5E2673 | 1.5px | #E6C5EC |
| disabled (any) | `button.disabled.bg` (p300 @ 20%) | none | `button.disabled.label` (p500 @ 50%) |

- **Focus ring:** drawn outside the pill as a `StadiumBorder` with `strokeAlignOutside`. Width is 2 (1.5 on small). Outset is 3.5 for large primary and for destructive, 3 for other primary sizes, 2 for secondary/ghost.
- **Mobile usage:** primary CTAs are full width (`expand: true`) and pinned to the bottom with padding `(16, 0, 16, 12)`. A secondary/ghost "Cancel" sits 12 below.

Related small buttons:
- `ReviewButtonsStack` (`review_buttons_stack.dart`): primary + ghost, `minWidth` 196, 12 gap. This is the desktop review variant.
- `AppIconHoverButton` (`app_icon_hover_button.dart`): 32 circle icon button with a hover fill.
- `AppTappable` (`app_tappable.dart`): Semantics(button) + click cursor + opaque GestureDetector. Use it for any custom tap target.
- `_ModalCloseButton` (inside `app_mobile_sheet.dart`): a 32×32 circle filled `button.secondary.bg`, with `AppIcons.cross` 20 in `icon.accent`. Semantics "Close".
- Inline field pills ("Paste" / "Clear" / "Max"): 36h, `button.secondary.bg`, `AppRadii.full`, horizontal padding 12–16, `labelLarge` in `button.secondary.label`.

### 3.2 Inputs

**`MobileTextField`** (`widgets/mobile_text_field.dart`) is the canonical mobile input.

```dart
MobileTextField({required TextEditingController controller, required FocusNode focusNode,
  Key? fieldKey, String? hintText, onChanged, onSubmitted, textInputAction, keyboardType,
  inputFormatters, Widget? leading, Widget? trailing, Color? backgroundColor,
  Color? restingBorderColor, Color? focusedBorderColor, List<BoxShadow>? focusedBoxShadow,
  TextStyle? textStyle, TextStyle? hintStyle, double? height, double? radius,
  bool enabled = true})   // new: disabling also unfocuses
```

Since `ff02152` the whole shell is a translucent `GestureDetector`: a tap outside the inner text region focuses the field and puts the caret at the end.

Visual spec:
- Height 60 and radius 16 (from `AppInputSizing`), fill `surface.input.primary` (#1B1F1F dark / white light).
- **No border at rest.** A 1.5px `background.inverse` border (`strokeAlignInside`) appears when focused.
- Surface shadow always. The inner `TextField` uses `InputDecoration.collapsed` with 12px horizontal padding.
- Text is `labelMedium` w500 in `text.accent`; hint is `labelMedium` in `text.muted`; cursor is `text.accent`.
- `leading` and `trailing` are laid out inside the row. A leading icon slot is `AppInputSizing.iconWrapWidth` (36) wide.
- Error state: pass `restingBorderColor` / `focusedBorderColor: colors.border.utilityDestructive`.
- **"Focus spotlight" trick** (send recipient): `focusedBoxShadow: [BoxShadow(color: neutralScrim, offset: (0,4), blurRadius: 4, spreadRadius: 1000)]` dims the whole screen around the focused field.

**`AppTextField`** (`widgets/app_text_field.dart`, 1044 lines) is the full-featured field. It is desktop-first but works on mobile.
- Label row (16h) with optional `rightLabel` / `rightSlot`, shell, and message row below.
- Tones: `AppTextFieldTone.neutral | destructive | success | brandCrimson`. Each changes the border, message color and default message icon (`warning` / `checkCircle` / `shieldKeyhole`).
- Surfaces: `AppTextFieldSurface.primary` (white `surface.input` + shadow) or `secondary` (filled `button.secondary.bg`, flat).
- Hover border is `border.subtleOpacity`; focus border is `background.inverse`.
- Multiline mode has a fixed 148 shell height and a custom 6px scrollbar.
- Also: clear button, `obscureText`; new `inlinePrefixText/Style`, `inlineSuffixText/Style`, `inlineAffixGap = 4`, `borderColor`.

Other inputs:
- `PasswordTextField` (`password_text_field.dart`).
- `CommaToDotInputFormatter` (`comma_to_dot_input_formatter.dart`).
- **`DecimalAmountInputFormatter({required int maxFractionDigits, int? maxLength})`** (`widgets/decimal_amount_input_formatter.dart`, new) replaces `ZecAmountInputFormatter`, which was removed from `zec_amount.dart`. It rejects invalid edits and turns a committed leading "." into "0." (PR #678). Send uses ZEC 8 decimals / 17 characters and USD 2 / 12, after `CommaToDotInputFormatter`.
- **`MobileNumericKeyboardToolbar({required child})`** (`widgets/mobile/mobile_numeric_keyboard_toolbar.dart`, new): a native "Done" bar on every iOS number pad (`ios/Runner/NumericKeyboardHandler.swift`, channel `numeric_keyboard`), mounted app-wide in `lib/app.dart`.

### 3.3 Cards and surfaces

| Widget | File | Spec |
|---|---|---|
| `MobileSurfaceCard({child, padding = all(16), cornerRadius = 20})` | `widgets/mobile/mobile_surface_card.dart` | `background.ground` fill, no shadow. Screens usually pass `cornerRadius: AppRadii.large` and `padding: EdgeInsets.fromLTRB(16, 32, 16, 32)`. |
| `ReviewWrapCard({children, surfaceColor, padding = kReviewWrapCardPadding, mainAxisSize = max})` + `ReviewWrapDivider` | `widgets/review_wrap_card.dart` | Full width, radius 24, `ground`, `appSurfaceShadow`, padding v24 / h16, children 16 apart. Divider is 1px `border.regular`. |
| `AppCarousel({required List<AppCarouselItem> items, initialPage = 0, autoplay = true, autoplayInterval = 5s, transitionDuration = 400ms, semanticLabel = 'Information', onPageChanged})`, `AppCarouselItem.icon(message, tileColor, icon, iconSize = 20)` / `.image(message, tileColor, imageAsset)` (new) | `widgets/app_carousel.dart` | Information carousel with autoplay, keyboard and accessibility support; used by the migration status guidance. |
| `AppModalCard` / `AppModalActions` | `widgets/app_modal_card.dart` | Desktop modal card, radius 24, modal shadow. |
| Home balance card | private `_BalanceCard` in `mobile_home_screen.dart` | See 4.4. |
| Option / radio card | private in theme sheet, Keystone select-account, endpoint | Height 64, `ground`, radius 16. Border 1.5 `border.strong` when selected, else 1 `border.subtle` (or none). Leading icon 20 (50% opacity when unselected). Label `bodyMediumStrong`. 24px radio on the right: selected = `background.inverse` circle with `check` 14; unselected = `raised` / `neutralSubtleOpacity` circle. |

### 3.4 List rows

- **`MobileListRow`** (`widgets/mobile/mobile_list_row.dart`)
  - Constructor: `MobileListRow({required String label, Widget? leading, String? value, Widget? trailing, bool showChevron = false, VoidCallback? onTap, bool enabled = true, Color? labelColor, valueColor, chevronColor, double minRowHeight = 52, TextStyle? textStyle, valueTextStyle})`.
  - New `Widget? labelBadge`, drawn 4px after the label (e.g. `SettingsNewBadge()`, a "New" pill from `features/settings/widgets/settings_new_badge.dart`).
  - Layout: leading, 12 gap, label (`bodyMedium`, `text.accent`), then either an expanded right-aligned value (`text.secondary`, ellipsis) or an expanded label. Then 8 gap and either `trailing` or `chevronForward` 16 (`icon.muted`).
  - Disabled rows use `text.disabled` / `icon.disabled` and ignore taps.
  - Settings rows override `minRowHeight: 44`, use `labelLarge` w400, value in `text.accent`, and chevron in `icon.accent`.
- **Activity row** `ActivityFeedRow` (`features/activity/widgets/activity_feed.dart`): see 4.7.
- **`ReviewListRow`** (`widgets/review_list_row.dart`): the 32px "List Item" row used in review/status detail cards. Left label; right value cluster with optional 16px leading/trailing icons (status icon, expand, explorer link, fee help).
- **`ReviewInfoRow`** (desktop) / **`MobileReviewInfoRow`** (`widgets/mobile/mobile_review_row.dart`; new options `TextStyle? valueStyle`, `double rowHeight = 90`, `bool reserveBottomRow = true`; Pay uses 76 and `headlineMedium`)
  - The 90px min-height "serif review row": 40px leading slot, 16 gap, then a column:
    - label: 24h, `labelMedium`, `text.secondary`
    - 4 gap
    - value: `headlineLarge` Young Serif 32, `text.accent`, one line, optional strikethrough
    - 4 gap
    - 24h bottom strip (pool badge, address, "Show full address" action)
  - Companions:
    - `MobileReviewFlowArrow`: 40-wide column with `arrowDown` 24 in `icon.accent`.
    - `MobileReviewZecBadge`: 32px round ZEC coin.
    - `MobileReviewIconBadge`: 40px circle in `neutralSubtleOpacity`.

### 3.5 Sheets and modals (`core/layout/mobile/app_mobile_sheet.dart`)

- `Future<T?> showAppMobileSheet<T>({required context, required WidgetBuilder builder, bool isDismissible = true, bool enableDrag = true, bool transparentBackground = false})` (`enableDrag` is new)
  - **iOS (new):** when `transparentBackground` is false it pushes a `PreparedModalSheetRoute` (`layout/mobile/prepared_modal_sheet_route.dart`, a `ModalBottomSheetRoute` that lays out offstage until the corner radius is known, then runs the normal entrance) on the root navigator instead of calling `showModalBottomSheet`.
  - Wraps `showModalBottomSheet(isScrollControlled: true, useSafeArea: true, useRootNavigator: true, backgroundColor: transparent, elevation: 0, barrierColor: colors.background.neutralScrim)`.
  - `useRootNavigator: true` makes the sheet cover the floating tab bar.
  - It re-provides `AppTheme` and the Riverpod `ProviderScope` container inside the sheet via `UncontrolledProviderScope`.
- **`MobileModalCard`** is the floating-card frame, not an edge-attached sheet:
  - 16px side margins.
  - Bottom gap: **16** on iOS (the home indicator floats inside it); 16 + `viewPadding.bottom` on Android; `keyboardInset + 16` when the keyboard is open. It was 32 at `ff02152` (PR #729); now exposed as `MobileModalCard.bottomGapFor(context)`. New `EdgeInsets? margin` (centred dialogs pass zero) and `bool followsScreenCorners = true`.
  - Corners: on iOS, continuous corners (`appModalShape` in `widgets/app_modal_shape.dart` → `RoundedSuperellipseBorder`), fixed 32 at the top and at least 32 at the bottom, adapted to the display's corner radius by `MobileModalCorners` (`layout/mobile/mobile_modal_corners.dart`, native `com.zcash.wallet/modal_corners` via `services/native_modal_corners.dart`, 250 ms animation, falls back to 32 with the keyboard up). Elsewhere all four corners radius 32 (`AppRadii.xLarge`). `background.base` fill, modal shadow, inner rim highlight (now painted along the shape path).
  - New `MobileModalOverlay({required background, required child})`: scrim plus a bottom-aligned card for inline use (previews, full-screen routes).
  - `transparentBackground: true` applies only the margins.
- **`MobileModalScaffold`** is the sheet content layout:
  - Constructor: `MobileModalScaffold({required String title, required VoidCallback onClose, required Widget child, Widget? leading, TextStyle? titleStyle, int titleMaxLines = 1, bool showTitle = true, bool showClose = true, double bodyGap = 16, double bottomPadding = 24, bool constrainBody = false})` (`constrainBody` wraps the child in `Flexible` so it can scroll).
  - Padding: top 32, sides 16, bottom 24.
  - Title row: min height 26, 40px right inset for the close button. Title is `bodyLarge` w600 in `text.accent`, with an optional leading widget and 12 gap.
  - Then `bodyGap`, then child.
  - Close button is absolutely positioned at `top: 15.5, right: 16`.
- `MobileSheetCancel`: a centred "Cancel" text action at the bottom of confirm sheets.
- Examples:
  - `mobile_tx_fee_info_sheet.dart`: `showMobileTxFeeInfoSheet(context, {String title = 'Tx fee', String description = ...})`; padding `(16,32,16,32)`, title in `headlineSmall`, body, secondary "Close" button.
  - `mobile_address_verify_sheet.dart`: now the public `MobileAddressVerifySheet({title, address, onClose, leading})` showing the full address; see 3.10.
  - `unsupported_sheet.dart`.
- Full-screen transparent overlays (Keystone signing) use a `CustomTransitionPage(opaque: false, barrierDismissible: false, barrierColor: neutralScrim, transitionDuration: Duration.zero)` route, with a `MobileModalCard` bottom-aligned inside it.
- Confirm-sheet pattern (destructive): title with a leading icon, body in `bodyMedium` `text.secondary`, destructive `AppButton` (optionally with a countdown label such as "Reset after 5s..."), then `MobileSheetCancel`.

### 3.6 Tab bar: `AppMobileTabBar` (`core/layout/mobile/app_mobile_tab_bar.dart`)

- API: `AppMobileTabBar({required List<AppMobileTabItem> items, required int currentIndex, required ValueChanged<int> onSelect})`, with `AppMobileTabItem({iconName, label})`. The label is for accessibility only; the bar is **icon-only**.
- **Pill:** height 64, `AppRadii.full`, floating 16px from the screen sides.
- **Glass:** `BackdropFilter(ImageFilter.blur(17.5, 17.5))` over `colors.macosUtility.navPanel` (#1A1A1A @ 30% dark / white @ 30% light). A hairline ring (`macosUtility.thinBorder`, spread 1) and a soft drop shadow.
- **Items:** padding 4, equal-width items 56 tall, icons 28. Inactive icons use `icon.muted`; the active icon uses `navPanel.activeIcon` (crimson #A83861).
- **Active pill:** one shared pill slides between items with `AnimatedAlign`. Fill `navPanel.activeBg` (crimson @ 15% dark / 10% light), 1px `border.subtleOpacity`.
- **Haptics:** `AppHaptics.auxiliaryKey()` on tap.
- Selecting the active tab again resets that branch to its root.

### 3.7 Top nav: `MobileTopNav` (`core/layout/mobile/mobile_top_nav.dart`), height 72

- **`.account(accountName, balanceLabel?, syncLabel?, syncLabelColor?, syncIndicatorColor?, syncAnimated, syncHighlightColor?, avatar?, onAccountTap?)`** (tab roots)
  - Left inset 16, 40px avatar, 12 gap, name in `labelLarge` w600 `text.accent`, optional balance line in `labelLarge` w400 secondary.
  - Right: sync label (`labelMedium`, `sync.text` green) plus a 6×50 glowing edge bar hugging the right screen edge. It shimmers and breathes while syncing.
  - Labels (`core/formatting/sync_status_label.dart`, `SyncStatusLabel.from(sync, {displayWholePercentage, networkPrivacy})`): "Vizor is synced", "NN% Syncing..." (clamped 0–99), "Syncing failed. <reason>...", plus new Tor states "Connecting to Tor…" and "Tor couldn't connect...", and "Sync paused" states.
  - Wrapped by `MobileTopNavAccount` (`mobile_top_nav_account.dart`), which binds it to providers.
- **`.back(title, onBack?, trailing?, backIcon = chevronBackward, titleStyle?, height = 72, titleMaxLines = 1, foregroundColor?)`** (pushed screens; the last two are new, `foregroundColor` tints the title and back icon)
  - A 44×44 back button at left 12 (icon 24 in `icon.accent`).
  - Centred title in `headlineLarge` (Young Serif 32), `text.accent`, inset by 56 on both sides.
  - Optional trailing widget at right 12. `onBack: null` hides the back button.
- **`.steps(progress, onBack?, showBackButton = true)`** (onboarding/wizards)
  - Centred 196×6 track in `background.overlay`, filled with `background.inverse`, full radius.
  - Back button at left 12.

`MobileTopScrollFade(child, height = 32)`: a window-colored gradient overlay at the top of a scroll view. It is transparent at rest and fades in over the first 24px of scroll, using smoothstep-sampled stops.

### 3.8 Feedback: toasts, haptics, copy

- **Toast** (`widgets/app_toast.dart`)
  - `showAppToast(context, String message, {Duration duration = 2s, String iconName = AppIcons.checkCircle, AppToastTone tone = AppToastTone.neutral})`. `enum AppToastTone { neutral, destructive }` is new.
  - Visual: a pill on `background.inverse`, radius 12, padding h12 / v8. Leading icon **20** (was 16) in `icon.inverse`, 4 gap, message in `labelLarge` `text.inverse`, at most 2 lines, centred. Destructive tone: `background.utilityDestructiveStrong` fill, white icon and text, label weight 400.
  - Position: top-centred at `max(32, safeTop + 8)`, horizontal padding 16.
  - Needs an `AppToastHost(child)` ancestor. `AppMobileShell` and each full-screen route scaffold add one.
  - Fallback: when called from inside a modal, it renders into the root overlay and re-provides `AppTheme`.
  - Warnings pass `iconName: AppIcons.warning`.
- **`copyTextWithToast(context, text:, toastMessage:)`** (`app_copy_feedback.dart`).
  - For secrets, use `copySensitiveText` in `core/clipboard/sensitive_clipboard.dart`. It expires after 1 minute on iOS (native pasteboard `expirationDate` + `localOnly`) and, since `ff02152`, on Android too (`IS_SENSITIVE` extra + auto-clear, `SensitiveClipboardHandler.kt`); desktop gets a plain copy.
- **`NetworkFallbackToastHost`** (`network_fallback_toast.dart`): the toast shown on endpoint failover.
- **`AppHaptics`** (`core/feedback/app_haptics.dart`):

  | Method | Feedback | Used for |
  |---|---|---|
  | `digit()` | lightImpact | passcode digit |
  | `auxiliaryKey()` | selectionClick | delete key, tab switch |
  | `privacyToggle()` | mediumImpact | hide-balance eye |
  | `copy()` | lightImpact | copying secrets |
  | `error()` | native error channel `com.zcash.wallet/haptics`, else 2× heavyImpact 90ms apart | wrong passcode |
  | `sendSuccess()` (new) | native `sendSuccess`: 30 ms pulse, then 40 ms full pulse after 60 ms; non-iOS fallback medium → 160 ms → light → 110 ms → selection | send status "Sent!" |
  | `sendFailure()` (new) | native `sendFailure`: four short pulses over 290 ms; fallback lightImpact | send status "Send failed" |
  | `votingAnswersComplete()` (new) | lightImpact | last ballot answer |

  At `ff02152` the send and Keystone flows used no haptics; the redesigned send status screen now fires `sendSuccess` / `sendFailure` (PR #314).

### 3.9 QR display

- **`DotQrShape`** (`widgets/dot_qr_shape.dart`): a `PrettyQrShape` for `pretty_qr_code ^3.6.0`.
  - Data modules are circular dots (`PrettyQrSmoothSymbol(roundFactor: 1)`).
  - Finder patterns are custom: a ring 1 module thick (7-module outer diameter) plus a 3-module centre dot.
  - `finderReferenceDimension` keeps dense codes proportional.
- **Receive QR:**
  - `QrImage(QrCode.fromData(data, errorCorrectLevel: M))`, rendered once to a 1536px bitmap and displayed with `RawImage`.
  - `PrettyQrDecoration(quietZone: zero, shape: DotQrShape(...), image: PrettyQrDecorationImage(position: embedded, scale: 54.26/size))`, with a centre logo badge.
  - Size 260 inside a 292×308 container, radius 32, 1px border.
  - Shielded QR: light dots on a dark surface in both themes. Transparent QR: dark dots on white in light mode.
- **Keystone QR (scan-optimized)** (also used for the receive QR of Keystone accounts since PR #762; software and Ledger accounts keep the dot style):
  - `PrettyQrView` with `PrettyQrSquaresSymbol(color: black)` (plain squares).
  - `errorCorrectLevel M`, `quietZone: modules(3)`, on `ColoredBox(colors.surface.qrCode)` (always white).
  - AGENTS.md says never to apply decorative QR treatments to codes a Keystone device must scan.
- **Scanner:** `mobile_scanner` (Chainapsis fork) behind `lib/src/services/qr_scanner.dart`.
  - Scan window `min(shortestSide × 0.9, 280)`.
  - Viewfinder: `MobileScanViewfinderCorners(cornerLength: 56, cornerRadius: 32, strokeWidth: 4)` over a `0xB3000000` scrim with a 256px clear window (radius 32).
  - Address scanning: `features/address_scan/widgets/mobile_address_scan_card.dart` (`MobileQrScanCard`).

### 3.10 Amount and address display

- **Amounts** (`core/formatting/zec_amount.dart`)
  - `formatZecAmount(BigInt zat, {minFractionDigits})`, `parseZecAmount(String)` / `ZecAmount.tryParse`, `ZecAmount.fromZatoshi(z).<preset>`. Presets return `ZecAmountPretty{amountText, denomText}`:

  | Preset | Output |
  |---|---|
  | `.balance` | `"0.00"`, `"1.25"` (min 2, max 8 decimals, trailing zeros trimmed, no ticker) |
  | `.fee` | `"0.0001 ZEC"` |
  | `.activity` | `"1.2345 ZEC"` (max 4 decimals; full 8 when 0 < v < 0.01) |
  | `.signedActivity` | `"+1.25 ZEC"` / `"-0.5 ZEC"` |
  | `.activityDetail` | `"1.00 ZEC"` |
  | `.receipt` | `"1.50 zec"` |
  | `.compactBalance` (new) | `compactBalancePretty({min 2, max 5, hideZeroFraction: true})`: fraction digits shrink as the integer part grows, `"12"` instead of `"12.00"`, values below 0.00001 show `"<0.00001"`. Home balance and the transparent strip use it (PR #593). |

  - `.pretty()` gained `hideZeroFraction`.

  - The ticker is `kZcashDefaultCurrencyTicker`: "ZEC" on mainnet, "TAZ" on testnet/regtest.
  - Mobile activity compaction (`features/activity/activity_amount_text.dart`): at most 14 characters, `"12.34K ZEC"`, `"1.234M ZEC"`.
  - Fiat: `$12.34`, `$1.5K`, `$1.234M`. The 24h change is `"+ 1.23% (24h)"` in `text.positiveStrong`, or `"- 0.45% (24h)"` in `text.destructive`.
- **Privacy masking** (`core/privacy/privacy_mask.dart`)
  - `fixedPrivacyMask()` returns `"******"`.
  - `hideAmountIfPrivacyMode(text, enabled, denomination)` returns `"****** ZEC"`. Activity rows use a 3-star mask.
- **Addresses** (`core/formatting/address_display.dart`)
  - `truncatedAddress(a)`: first 7 + `" ... "` + last 6 (unchanged if 18 characters or fewer).
  - `truncatedTxid(t)`: 8 + `"..."` + 8.
  - `zcashAddressDisplayKind(a)`: `t…` means transparent.
  - `addressVerifyGrid(a)`: 5-character groups, 5 per row. Groups 0, 2, N−3 and N−1 are flagged for crimson highlight. **Kept for legacy callers only**; the verify sheet no longer uses it.
  - **Full-address viewer** (new, `widgets/full_address_viewer.dart`, PR #644): `FullAddressText({address, color})` renders one continuous soft-wrapped string in `codeMedium` (Geist Mono) `text.primary`; `FullAddressCopyButton({address, expand = false, size = mediumLarge, label = 'Copy address'})`; `copyFullAddress()` toasts "Address copied". `MobileAddressVerifySheet` = title, scrollable `FullAddressText`, 24 gap, full-width large "Copy address", header close (the chunk grid and "Cancel" are gone).
  - `PoolBadge({required bool isShielded})` (new, `widgets/pool_badge.dart`): icon + "Shielded" / "Transparent", crimson or muted glyph, `text.secondary` label.
  - Contact names on every address surface (PR #313): `features/address_book/models/address_book_label_lookup.dart` + `widgets/contact_name_inline.dart`, exact match only, never picking between duplicate contacts.
  - Local variants: receive screen 13 + `" ... "` + 11; contacts 6 + 5; send review 7 + `" .... "` + 7.
  - Truncated addresses use Geist `labelLarge` / 14/16 w500. **Full addresses are now Geist Mono** (the full-address viewer); Geist Mono is also used for mnemonic indices and code chips.

### 3.11 Loading, skeletons, empty states

- **Spinner:** `AppIcon(AppIcons.loader)` (the animated spoke loader) at 16–20px. It is also used as the leading icon of pending activity rows and the "In progress" status chip.
- **Shimmer bars:** a pill of `background.overlay` @ 0.15 with a sweeping `background.raised` highlight (1200ms). The fiat-price placeholder is now the shared `AmountPriceLoadingBar({animated = false, width = 48, height = 12})` (`widgets/amount_price_loading_bar.dart`), static under reduced motion.
- **`AppLoadingIcon`** now stops animating when `MediaQuery.disableAnimations` is set.
- **`MobileTransactionProgressScreen`** (new, `widgets/mobile/mobile_transaction_progress_screen.dart`): `({required MobileTransactionProgressPhase phase /* inProgress | pending | succeeded | failed */, required String title, required String body, required bool canPop, onPopBlocked, primaryActionLabel, onPrimaryAction, secondaryActionLabel, onSecondaryAction, double? bodyMaxWidth = 223})`. Full-page background `assets/illustrations/mobile_send_status_background.png`, a `MobileTransactionProgressBadge` (loader, success with ripple, or failure), title in `displayLarge`, body in `bodyMediumStrong`, buttons. Used by send status (4.6) and `pay/screens/mobile/mobile_pay_submitted_screen.dart`.
- **QR placeholder:** a radius-24 square with a vertical band sweep (1400ms): `raised` base, `overlay` @ 0.72 highlight.
- **Empty states:** illustration, then title (`headlineSmall` or `headlineLarge`, `text.accent`), then 4 gap, then body (`bodyMedium` `text.secondary`, centred, max width about 236–250), then optional CTA 32 below. Examples:
  - Home: "No activity, yet..." / "How about running your\nfirst ZEC tx?"
  - Contacts: "No contacts yet" / "Add your first contact to get started."
- **Message cards** (activity list): a 160-tall section card with centred `labelLarge` `text.secondary`: "No activity yet", "Loading activity...", or "Couldn't load activity. Try again in a moment." (the error in `text.destructive`).
- **Sensitive overlay:** `SensitivePrivacyOverlay(sensitiveContentVisible:, child:)` (`core/privacy/sensitive_privacy_overlay.dart`) blurs the child (sigma 30, #141818 @ 20% scrim) and shows a 98×98 lock badge when the app is backgrounded or being captured. It wraps the seed phrase and viewing-key screens. Capture blocking is toggled via `com.zcash.wallet/privacy_shield`: Android `FLAG_SECURE`, and since PR #689 also iOS (`SecureScreenshotShield`) and macOS (6.4).
- **Sync keep-awake hosts** (new, `widgets/mobile/sync_keep_awake_{native_host,privacy_lock_host,interaction_listener}.dart`), mounted in `app.dart` around the router: `SyncKeepAwakeNativeHost({child, bridge, retryDelays})`, `SyncKeepAwakePrivacyLockHost({child, idleTimeout = 1 min})` with `SyncKeepAwakePrivacyLockScreen`, `SyncKeepAwakeInteractionListener({child})`. Copy in 6.4.

### 3.12 Misc

- `AppChip`: a mnemonic word chip, min height 26, padding 4, optional `codeSmall` index and `labelLarge` label, default fixed width 80.
- `AppContextMenu`: desktop.
- `AppTooltip`.
- `AppDecorativeDivider`: a 256×16 ornament divider for onboarding.
- `AppProfilePicture` / `AppProfilePicturePickerModal`.
- `BiometricIcon({required BiometricKind kind, double? size, double? fingerprintSize})` (new): Face ID / Touch ID / fingerprint artwork.
- **`MobileAccountAvatar`:** a profile picture plus, for hardware accounts, a **signer badge**. `isHardware` was replaced by `HardwareSignerKind? hardwareSignerKind`: Keystone shows `AppIcons.keystone` 12, Ledger `AppIcons.ledger` 16. The badge is a 20×20 rounded square (radius 6) in `background.inverse` with a 2px ring in the color of the surface underneath, `AppIcons.keystone` 12 in `icon.inverse`, positioned at right −4 / bottom −2. *Zafe could use the same badge to mark a FROST vault.*

---

## 4. Mobile shell, navigation and screens

### 4.1 Shell: `AppMobileShell` (`core/layout/mobile/app_mobile_shell.dart`)

```dart
Scaffold(
  backgroundColor: colors.background.window,
  extendBody: true,                       // content scrolls under the floating bar
  body: AppToastHost(child: body),
  bottomNavigationBar: MobileBottomSafeArea(
    bottomPadding: bottomGap,             // iOS 16, Android 12
    child: Padding(padding: EdgeInsets.fromLTRB(16, 0, 16, bottomGap), child: tabBar)));
```

`MobileBottomSafeArea(bottomPadding, child)` handles the iOS home indicator:
- On iOS, it skips the bottom inset when `bottomPadding >= 16`, because the home indicator is an overlay.
- On Android, it always applies the inset.
- The same rule applies to sheets and pinned CTAs.

Tab root content needs its own bottom scroll padding: `64 (bar) + 48`, so the last row clears the floating bar.

`app_mobile_shell.dart` and `app_mobile_tab_bar.dart` are unchanged since `ff02152`. `lib/app.dart` now wraps the router in app-wide hosts: `SigningShutdownHost`, `_IncomingLinkHost` (payment and gift links), `SyncKeepAwakeNativeHost`, `SyncKeepAwakePrivacyLockHost`, `SyncKeepAwakeInteractionListener`, `PaymentRequestHost` (the ZIP-321 card sits above the router, 12.2) and `MobileNumericKeyboardToolbar`. Android root back now needs a second press: `core/navigation/mobile_exit_back_guard.dart` shows "Go back again to exit" and exits within 2 s (PR #307).

**Pushed full-screen route pattern** (send, receive, settings detail):

```
Scaffold(backgroundColor: background.window)
 → AppToastHost
 → SafeArea
 → Column[ MobileTopNav.back(title, onBack), Expanded(body), pinned CTA area ]
```

### 4.2 Router (`lib/src/core/navigation/mobile_routes.dart`, `mobile_onboarding_routes.dart`; wired in `lib/app.dart`)

`go_router ^17.1.0`. `buildMobileRoutes(entryRoutes: [...appAuthRoutes(ref, bootstrap, unlockScreen: MobileUnlockScreen()), ...mobileOnboardingRoutes()])`:

- **Tab shell.** `StatefulShellRoute.indexedStack` (page = `CupertinoPage(_MobileTabShell)`).
  - Branches: `/home` (plus `/accounts` in the same branch, keeping the tab bar), `/swap`, `/activity`, `/settings`.
  - Tab pages use `NoTransitionPage`.
  - `_allMobileTabs` is a single const list driving both the branch order and the tab-bar order.
  - Hidden tabs (swap when disabled) are filtered out while indices are remapped.
  - `mobilePreviousTabPathProvider` records the outgoing tab so a tab root can offer a "back to previous tab" action.
- The tab set is unchanged: still `/home`, `/swap`, `/activity`, `/settings` (3 or 4 visible tabs depending on the swap flag). The swap-flag fetch now goes through the Tor-aware client (20 s timeout; unknown answers retry at 15 s, 30 s, 60 s, 120 s, then every 300 s), and the flag also gates `/pay/*`.
- **Full-screen pushes over the shell** (`CupertinoPage`, which hides the tab bar):
  - Settings detail: `/settings/seed-phrase`, `/settings/address-book`, `/settings/endpoint`, `/settings/change-password`, and new `/settings/viewing-key`, `/settings/explorer`.
  - Send: `/send` (extra: `SendPrefillArgs` from a ZIP-321 request, with address, amount, memo and label, or a prefilled recipient `String`; a payment URI opens on the amount step), `/send/amount` (extra `MobileSendAmountArgs`), `/send/review` (extra `MobileSendReviewDraftArgs`), `/send/status` (extra `SendReviewArgs` | `KeystoneBroadcastArgs` | `LedgerBroadcastArgs`). `/send` and `/send/review` use `payloadScopedPageKey` (`payload_page_key.dart`) so a new payload builds a new page.
  - Swap: `/swap/review`. Pay (new): `/pay`, `/pay/review`, `/pay/submitted/:intentId`.
  - Activity: `/activity/tx/:txid?kind=` (the list now loads `getTransactionDetail` first and passes `extra: MobileTransactionStatusArgs`), `/activity/swap/:swapId`.
  - Home: `/home/keystone-shield`, `/home/ledger-shield` (new).
  - New features: `/payment-links` (Gift Cards), `/voting`, `/voting/poll/:roundId` (+ `/review`, `/status`, `/submitted`, `/results`), `/migration`, `/migration/{intro,how-it-works,options,complete}`, `/migration/private/{notifications,start,status,schedule,preparation-schedule}`, `/migration/private/keystone/{sign,denominations/sign,batch/sign}`, `/migration/immediate/keystone/sign`, `/migration/fast/review`.
  - Other: `/receive`, `/about`.
  - A route that receives the wrong `extra` type falls back to the first step, e.g. `MobileSendScreen`.
- **Hardware signing routes:** `/send/keystone-sign`, `/swap/keystone-sign`, and new `/send/ledger-sign`, `/swap/ledger-sign`. (The Keystone flow is now a full-screen two-step layout, 5.3.)
- **Onboarding** (all `CupertinoPage`):
  - `/welcome`, `/add-account`, `/onboarding/method`, `/onboarding/intro`, `/onboarding/address-types`, `/onboarding/things-to-know`, `/onboarding/secret-passphrase`.
  - `/onboarding/set-passcode` redirects to `/welcome` unless `extra is SetPasswordScreenArgs`.
  - `/onboarding/biometrics`.
  - `/import`, `/import/manual`, `/import/birthday`.
  - `/onboarding/keystone`, `/onboarding/keystone/scan`, `/onboarding/keystone/select-account`, `/onboarding/keystone/birthday`.
  - New: `/onboarding/customise-account`, `/import/review`, `/onboarding/link-desktop` (+ `/scan`, `/accounts`, `/contacts`), `/onboarding/ledger`, `/onboarding/ledger/birthday`, `/onboarding/ledger/customise-account`.
- **Auth routes:** `/unlock`, `/lost-password`, `/storage-unavailable`, `/terms`, `/privacy`.
- **Redirect guard** `appRedirect` (now `appRedirect({ref, bootstrap, state})`, with shared predicates in `app_route_predicates.dart`: `isOnboardingLocation`, `isUnlockFlowLocation`), evaluated in order:
  1. Bootstrap blocking failure goes to `/storage-unavailable`.
  2. No wallet and not in onboarding goes to `/welcome`.
  3. Locked goes to `/unlock`.
  4. Unlocked while on unlock goes to `/home`.
  5. Wallet exists while on `/welcome` goes to `/unlock` or `/home`.
  6. Feature-flagged routes go to `/home`.
  - New rules: a wallet provider error means no redirect; `/storage-unavailable` recovers to `/welcome`, `/unlock` or `/home` once storage works; no wallet on an unlock-flow route or `/add-account` goes to `/welcome`; `/settings/uninstall` is exempt.
- **Incoming links** (new, 12.2): `incoming_link_dispatch.dart` (`classifyIncomingLink()` → PaymentRequest / GiftCard / VizorHome / Unknown; host checked before scheme), `vizor_deep_link.dart` (`VizorDeepLinkRoute {home, paymentLink}`, base `https://link.vizor.cash`, `VIZOR_DEEPLINK_BASE_URL`), `payment_uri_drain_policy.dart` (parks a URI for up to 10 minutes; delivery is a card over the current screen, not a navigation), `payment_uri_busy_surface_{provider,hold}.dart` (defers while a Keystone QR or send review is live), `payment_uri_unlock_claim.dart`, `route_stack.dart` (`isRouteTopmost(context)`).
- **Refresh:** `refreshListenable` is a `RouterRefreshController` (`ChangeNotifier`), poked by `ref.listen` on the wallet, security and feature providers. `pauseWhile(action)` defers redirects during multi-step mutations, so onboarding does not bounce mid-flow.
- `onException` falls back to `/home`. Paths are identical to desktop so deep links and the guard are shared.

### 4.3 Screen inventory (mobile)

| Area | Screen class | File |
|---|---|---|
| Entry | `MobileWelcomeScreen` | `features/onboarding/mobile/mobile_welcome_screen.dart` |
| | `MobileMethodSelectionScreen` | `.../mobile_method_selection_screen.dart` |
| | `MobileOnboardingIntroScreen`, `MobileAddressTypesScreen`, `MobileThingsToKnowScreen` | `.../mobile_create_steps.dart` |
| | `MobileSecretPassphraseScreen` (+ `SeedCard`) | `.../mobile_secret_passphrase_screen.dart`, `seed_card.dart` |
| | `MobilePasscodeScreen` (+ `PasscodePromptField`, `PasscodeNumpad`, `PasscodeBiometricButton`) | `.../mobile_passcode_screen.dart`, `passcode_widgets.dart` |
| | `MobileBiometricsScreen` | `.../mobile_biometrics_screen.dart` |
| | `MobileUnlockScreen`, forgot-passcode sheets | `.../mobile_unlock_screen.dart`, `forgot_passcode_sheet.dart` |
| | `MobileImportScreen`, `MobileImportManualScreen`, `MobileImportBirthdayScreen` | `.../mobile_import_*.dart` |
| | Keystone intro/scan/select/birthday | `.../mobile_keystone_screens.dart`, `mobile_keystone_scan_card.dart` |
| | Shared scaffold | `.../mobile_onboarding_scaffold.dart` (`MobileOnboardingStepScaffold`), `mobile_onboarding_progress.dart` |
| Tabs | `MobileHomeScreen` | `features/home/screens/mobile/mobile_home_screen.dart` |
| | `MobileActivityScreen` | `features/activity/screens/mobile/mobile_activity_screen.dart` |
| | `MobileSettingsScreen` | `features/settings/screens/mobile/mobile_settings_screen.dart` |
| Flows | `MobileSendScreen` / `MobileSendAmountScreen` / `MobileSendReviewScreen` | `features/send/screens/mobile/mobile_send_screen.dart` (4685 lines; 3824 at `ff02152`) |
| | `MobileSendStatusScreen` | `.../mobile_send_status_screen.dart` |
| | `MobileKeystoneSignScreen` | `.../mobile_keystone_sign_screen.dart` |
| | `MobileKeystoneShieldScreen` | `features/home/screens/mobile/mobile_keystone_shield_screen.dart` |
| | `MobileReceiveScreen` | `features/receive/screens/mobile/mobile_receive_screen.dart` |
| | `MobileTransactionStatusScreen` | `features/activity/screens/mobile/mobile_transaction_status_screen.dart` |
| | `MobileAccountsScreen`, `showMobileAccountsSheet` | `features/accounts/screens/mobile/`, `widgets/mobile/` |
| | `MobileAddressBookScreen` | `features/address_book/screens/mobile/` |
| | Seed phrase / change passcode / endpoint / viewing key / explorer | `features/settings/screens/mobile/` (`mobile_viewing_key_screen.dart`, `mobile_explorer_screen.dart` are new) |
| New | `MobileLedgerSendSignScreen`, `MobileLedgerShieldScreen` | `features/send/screens/mobile/mobile_ledger_send_sign_screen.dart`, `features/home/screens/mobile/mobile_ledger_shield_screen.dart` |
| | Customise account, import review, wallet link, Ledger connect/birthday/device sheet | `features/onboarding/mobile/mobile_{customise_account,import_review,wallet_link_screens,ledger_connect,ledger_birthday,ledger_device_sheet}*.dart` |
| | Payment request card, receive request sheet | `features/send/widgets/payment_request_{card,host,surface}.dart`, `features/receive/widgets/mobile/receive_request_sheet.dart`, `features/receive/widgets/request/*` |
| | Pay, migration (about 24 files), voting, Gift Cards | `features/pay/screens/mobile/`, `features/migration/screens/mobile/`, `features/voting/screens/mobile/`, `features/payment_links/screens/` |

`mobile_home_screen.dart` grew from 1072 to 2381 lines and `mobile_send_screen.dart` from 3824 to 4685 since `ff02152`.

### 4.4 Home (`MobileHomeScreen`)

Top to bottom, inside `SafeArea(bottom: false)` → `Column` (plus stacked overlay hosts: the keep-awake prompt sheet "Stay awake to sync?" with "Keep screen awake" / "Maybe later", and Ironwood migration announcement / attention / completion sheets, e.g. "Go to migration page" / "I’ll visit later"):

1. **`MobileTopNavAccount`**
   - Avatar 40, account name, sync label and edge glow on the right.
   - Tapping the account opens `showMobileAccountsSheet`.
2. **`MobileTopScrollFade` → `ListView`**, padding `LTRB(16, 16, 16, 64+48)` (was 12 at the top). An optional `_PrivateStatusCoverageNotice` with a "Settings" button can sit above the card.
3. **Balance card** (`_BalanceCard`):
   - **Outer container:** `background.ground`, radius 24, `appSurfaceShadow`, clipped.
   - **Inner dark card:** height 200, padding 16, `background.homeCard`, radius 24, 1.5px border white @ 7%.
   - **Top row:**
     - `shieldKeyhole` 20 in `text.homeCard`, 8 gap.
     - "Shielded balance" (Geist 14/16 w400).
     - Spacer, then the **privacy eye**: 32×32 circle in white @ 5%, `eye`/`eyeClosed` 16, Semantics "Hide balance" / "Show balance". It fires `AppHaptics.privacyToggle()` and toggles `privacyModeProvider`.
   - `Spacer()`.
   - **Fiat row** (only when a price is known): fiat in Geist 14 @ 80%, 8 gap, 24h change `"+ 1.23% (24h)"` (green / plum / neutral).
   - 8 gap.
   - **Balance** `Text.rich`: amount in Young Serif 45/48, −1.35, lining figures, then `" ZEC"` in Young Serif 32/33. One line with ellipsis.
     - Shielded balance = sapling + orchard (+ Ironwood) + pending of each, formatted with `compactBalance` (at most 4–5 decimals).
     - A "Migration required" pill appears (and the balance shows disabled) while funds still need the Ironwood migration.
     - Privacy mode shows `"******"`.
   - **Transparent strip** (`_MobileTransparentBalanceStrip`), only when transparent > 0:
     - 57 tall on the white outer container under the dark card, padding h16 / v8, animated in and out.
     - `transparentBalance` 20, then `"Transparent: 0.50 ZEC"` in Geist 14 w500 `text.primary`.
     - Right side: text button "Shield ›" (or "Shielding..." with a loader).
     - Software accounts shield inline, then toast "Shielding complete". Keystone accounts push `/home/keystone-shield`.
4. 12 gap, then the **action buttons**:
   - With a balance: `Row[Expanded(AppButton primary "Send", leading plane), 8, Expanded(AppButton secondary "Receive", leading arrowDownCircle)]`, height 50, plus (new) a 50×50 icon-only secondary **"Pay"** button (`AppIcons.paid`) when swap is enabled. Send is disabled while migration is required.
   - With zero balance: a single primary "Receive your first ZEC" (leading `addNew`).
   - There is no scan button.
   - New below the actions: `_MobileVotingEntry` (card on `ground`, radius 24, `vote` icon, "Coinholder voting" / "Help to shape the network"; shown only after positive discovery of an active poll) and `MobileIronwoodMigrationBanner` ("Migrate to Ironwood", "Migration in progress", "Preparing migration", "Waiting for confirmation...", "Continue your migration", "Next migration batch is ready", "Migration needs attention").
5. 24 gap, then **Recent activity** (padding h8 / v12):
   - Header: "Recent activity" (`labelLarge` w600) and a "See all ›" link (24h, `button.ghost.label`) → `context.go('/activity')`.
   - 24 gap, then up to 10 `ActivityFeedRowGroup` rows 12 apart, with date-only timestamps and no section cards. Gift-card and swap entries are merged in (`gift_card_activity_index.dart`).
   - Empty state: "No activity, yet..." (`headlineSmall`), "How about running your\nfirst ZEC tx?" (`bodyMedium` secondary), and a 340×220 character illustration.
6. **Importing state** (first sync after import):
   - A background illustration.
   - Bottom-aligned: big `"NN%"` in `displayLarge`, "We're importing your wallet..." (Young Serif 24/28), and "Hang tight ... It might take some time. Keep Vizor open & running."
   - The sync label is hidden during this state.

There is no pull-to-refresh anywhere. Sync is automatic: a 10s poll plus a lifecycle resume.

### 4.5 Receive (`MobileReceiveScreen`)

1. `MobileTopNav.back(title: 'Receive ZEC')`.
2. `ListView`, padding `LTRB(16, 12, 16, 24)`, then a 16.5 gap.
3. **`ReceiveTabs`** segmented control:
   - 320×44 stadium, `background.raised`.
   - The sliding indicator is `background.inverse` (always dark) with a 2px inset and 160ms animation.
   - Tabs: "Shielded" (`shieldKeyhole` 20) and "Transparent" (`transparentBalance` 20), in `labelLarge`. The selected label is `text.inverse`.
4. 32 gap, then a **PageView** (292×340) synced with the tabs. Each page:
   - `ReceiveQrSurface`: QR 260 in a 292×308 container, radius 32, 1px border, padding x16 / y24. Dot-style QR with a centre badge.
   - Shielded page only: **`ReceiveRenewButton`**, a 48px circle overlapping the QR's bottom edge at top 292.
     - `homeCard` fill, 2px `border.inverseOpacity` ring, 4px `macosUtility.window` halo, `renew` 24.
     - It generates a new diversified address. Semantics "Generate new shielded address".
5. 24 gap, then the address summary (288×70):
   - Account name (`bodyLarge` w600).
   - 4 gap, then the compact address (13 + " ... " + 11) in `labelLarge` `text.secondary`, with a 140ms fade on change.
   - A `help` 20 icon button (40 box) opens `showReceiveAddressInfoSheet`.
6. 32 gap, then the buttons (300 wide):
   - A row: primary (shielded) or secondary (transparent) `AppButton` "Share shielded address" / "Share transparent address", leading `share` (`share_plus`), 8 gap, then (new) a 50×50 icon-only secondary **"Request ZEC"** button (`qr`) that opens `showReceiveRequestSheet` (12.2).
   - 12 gap, then a text button (50h) with `copy` 20 and "Copy shielded address". Toast "Address copied".

Shielded receive addresses no longer include a Sapling receiver (PR #739, `rust/src/wallet/addresses.rs` `receive_address_request()`); older addresses are still recognized.

The info sheet has a title with a one-line subtitle, then bullet rows (icon 20, 12 gap, `bodyMedium`, 8 apart), then a secondary "Close" button. Shielded bullets:
- "Tx details — sender, receiver, and amount — are encrypted on-chain & hidden."
- "A new Zcash shielded address is generated only when you tap the renew button."
- "Each new address is a diversified address derived from the same key. They all receive to the same wallet."

### 4.6 Send (mobile wizard)

All three steps are one widget (`MobileSendScreen`), selected by route. `_SendStep {recipient, amount, review}`. A `sendFlowId` (random 16-byte hex) threads through all Rust calls.

Chrome for every step:
- `Scaffold(bg: window, resizeToAvoidBottomInset: true)` → `AppToastHost` → `SafeArea(Column[MobileTopNav.back(title), Expanded(body)])`.
- Titles: "Select Recipient" → "Enter Amount" → "Review Send" ("Review Payment" when answering a ZIP-321 request; or "Send failed").

**Step 1: Recipient**
- **Floating address field** at `top: safeTop + 72 + 12`, sides 16:
  - `MobileTextField(hintText: 'Zcash Address')`.
  - Leading `plane` 20 (`icon.regular` when empty, `icon.accent` when filled).
  - Trailing pill "Paste" / "Clear" (36h).
  - When focused, the spread-1000 scrim shadow dims everything; tapping outside unfocuses.
- **Validation** runs on each change via `rust_sync.validateAddress(address:)`, returning `addressType` 'unified' | 'sapling' | 'transparent' | 'tex' or invalid. A sequence number drops stale results.
  - Error row: `labelLarge` in `text.destructive`, and the field border turns `border.utilityDestructive`. Copy: "Invalid address", "Address validation failed", "Keystone does not support TEX sends yet."
- **Below the field:**
  - A scan row (44h): 40 disc with `qr` icon, "Scan a QR Code" / "Scan an address using camera".
  - Contacts: header `users` 20 + "N contacts". Rows are 44h with a 32 avatar and the label over the truncated address. Since PR #576 the list is **filtered live by the text typed in the address field** (`filterAddressBookContacts(query:, networks: {zcash})`); tapping a contact fills the address, and a valid address shows the matching contact or own-account name.
  - New error "This address is for a different Zcash network"; the Keystone TEX error moved to swap only (Keystone now signs TEX sends, PR #559).
- **CTA:** full-width primary at the bottom, padding `(16,0,16,12)`. Label is "Enter address to continue" when empty, otherwise "Continue".

**Step 2: Amount** (system decimal keyboard, autofocus; no custom numpad in code, although the design mock `22_send_amount_ready.png` shows one)
- Fixed 285h top block, padding `(16,16,16,0)`:
  1. **Balance row** (44h):
     - 40px ZEC coin (fixed `#F4B728`, white `zcashCurrency` 22).
     - Spendable amount (`labelLarge` w600).
     - "Max" pill on the right, Semantics "Use maximum spendable balance".
  2. 24 gap, then the **amount row** (64h, centred, baseline aligned):
     - Input in Young Serif at fontSize 48, `text.accent` (or `text.destructive` on error).
     - ZEC mode: the unit "ZEC" follows at size 38 with 50% alpha. USD mode: a "$" prefix.
     - The input width is measured with `TextPainter`. A custom iOS-style blinking empty cursor is drawn.
     - Hint "0" in `text.disabled`.
  3. 12 gap, then the **conversion toggle**: `doubleArrowVertical` 20 + "$ 12.34" or "0.1234 ZEC" in `labelLarge` secondary. Tapping swaps the input currency (helpers in `send/services/send_amount_conversion.dart`). `AmountPriceLoadingBar(animated: true)` shows while the price loads.
  4. 24 gap, then **"Sending to"**: label, then a 68h row with a 40 avatar and the contact name over the truncated address.
- **Formatters:** comma→dot, then `DecimalAmountInputFormatter`: ZEC 8 decimals / 17 characters; USD 2 decimals / 12 characters.
- **Validation:** fee via `rust_sync.estimateFee`. Error "Not enough ZEC". Max uses `rust_sync.estimateSendMax`, with errors "Max amount unavailable" and "Message is too long".
- **CTA label carries the state:** "Enter amount to continue" → "Calculating max amount" → "Not enough ZEC" (disabled) → "Finish & review".

**Step 3: Review**
- Padding `(16,12,16,12)`. Info block (268h, v24):
  - `_ReviewInfoRow` "Amount": ZEC coin, headline "123.12 ZEC" in Young Serif 32, fiat on the bottom line.
  - 8, then `arrowDownward` 24, then 8.
  - "To" row:
    - Leading: avatar 40, or a wallet disc.
    - Headline: contact name, or "Shielded address" / "Transparent address" / "TEX address".
    - Bottom line: `shieldKeyhole` 16 (crimson) or `transparentBalance` (muted) with the compact address (7 + " .... " + 7).
    - Ghost small button: `eye` + "Full address".
  - The full-address sheet is `showMobileAddressVerifySheet`, now the Geist Mono full-address viewer with "Copy address" (3.10). At `ff02152` it was a 5-character chunk grid (chunks 0, 2, N−3, N−1 in crimson) with a "Cancel" button; that design is gone.
- 32 gap, then the **detail card** (`MobileSurfaceCard`, radius 24, padding h16 / v32):
  - "Message" row (shielded recipients only):
    - Empty: `edit` + "Add short encrypted message".
    - Set: memo text with `doubleArrowVertical`.
  - A 16/1px/16 divider.
  - "Tx fee" row: `"0.0001 ZEC"` (or "—" before the estimate) plus `help` 20, which opens the fee sheet. Fee sheet body: "The network fee is set by the Zcash protocol (ZIP 317) based on the transaction size. Vizor adds no extra fee."
  - **Memo sheet** ("Add Memo"): header "Message" with a byte counter "51/512".
    - 148h text area (`ground`, radius 12, 1.5 inverse border when focused), hint "Only the recipient can read this".
    - Buttons: primary "Add Memo" (or "Clear memo" with `trash`), ghost "Cancel". Error "Message is too long".
- **Buttons:**
  - Primary "Confirm & Send" (leading `plane`), which becomes "Preparing..." while proposing; hardware accounts read "Confirm with Keystone" / "Confirm with Ledger", and a blocked sync shows "Finishing wallet sync...".
  - Ghost "Cancel", which does `go('/home')`.
- **Confirm:**
  1. `proposeSendTransfer` → `rust_sync.proposeSend(dbPath, network, accountUuid, sendFlowId, toAddress, amountZatoshi, memo)`, returning `SendReviewArgs{proposalId, sendFlowId, proposalAccountUuid, address, addressType, amountZatoshi, feeZatoshi, needsSaplingParams, memo}`.
  2. Software accounts: `go('/home')` then `push('/send/status', extra: args)`.
  3. Hardware accounts: `push<KeystoneBroadcastArgs>('/send/keystone-sign', extra: args)` or `push<LedgerBroadcastArgs>('/send/ledger-sign', ...)` (section 5).
- **In-screen failed phase:**
  - Warning icon 48 (destructive), then "Send failed" (`displayLarge`), then "`<amount>` to `<label>`".
  - Friendly error, one of: "Insufficient shielded balance to cover amount and fee." / "Network error. Check your connection and try again." / "The network rejected this transaction. Try again." / "Transaction expired before it could be sent. Try again." / "Send failed. Try again."
  - Buttons: "Try again" and the text link "Back to wallet".

**Status** (`MobileSendStatusScreen`), **rewritten since `ff02152`** (PR #317; 635 → 278 lines). It is now a `MobileTransactionProgressScreen` (3.11) over `mobile_send_status_background.png`; the review rows, detail card, status chip and Tx ID are gone:

| Phase | Title / body | Action |
|---|---|---|
| sending | "Sending..." / "Submitting your transaction to the network..." | back blocked |
| pendingBroadcast | "Queued to send" / "Your transaction was created and will be submitted automatically. Check the Activity page before sending again." | "Done" |
| succeeded | "Sent!" / "It will confirm on-chain shortly. Track it in Activity." | "Done"; `AppHaptics.sendSuccess()` |
| failed | "Send failed" / "Nothing was sent, your funds haven't moved. Try again." | "Return home"; `AppHaptics.sendFailure()` |

The in-screen compose failure on the review step ("Try again" / "Back to wallet") still exists. The rest of this subsection describes the `ff02152` status screen, kept because its detail-card vocabulary is still used by `MobileTransactionStatusScreen` (4.7) and by Zafe's proposal detail:
- Phases `{sending, pendingBroadcast, succeeded, failed}`. Titles: "Sending...", "Sent successfully", "Send failed". Back is blocked while sending.
- Layout: `MobileReviewInfoRow` Amount (ZEC badge 32), `MobileReviewFlowArrow`, `MobileReviewInfoRow` To (struck through on failure).
- 24 gap, then the detail card (`ground`, radius 24, padding h16 / v32) with 32h rows:
  - "Status" chip:
    - `loader` "In progress" (`text.secondary`)
    - `checkCircle` "Completed" (`text.positiveStrong`)
    - `cross` "Failed, funds returned" (destructive)
  - "Message" (18 characters + "..."), "Timestamp", "Tx ID" (truncated, `arrowTopRight` opens the explorer; on failure it copies with toast "Transaction Hash Copied"), divider, "Tx fee".
- Pending note (`bodySmall` secondary): "Transaction was created locally but could not be broadcast. It will retry automatically when the network is available. Do not send again unless this transaction expires."
- At `ff02152` there was no hero illustration or celebration; the current screen has a background illustration, a success ripple and haptics.
- **Sapling params gate** (non-dismissible sheet):
  - "Download Required" / "To create this private transaction, your wallet needs to download about 50MB of cryptographic parameters." / "This happens once, then it's done.\nNetwork data charges may apply."
  - Buttons: "Download" / "Cancel".
  - Params come from `https://download.z.cash/downloads/`, are SHA-1 verified and written to `<appSupport>/sapling_params/`.

### 4.7 Activity

**`MobileActivityScreen`**
- `MobileTopNav.back(title: 'Activity')`.
- `ListView`, padding `LTRB(4, 12, 4, 112)`, containing `ActivityFeed(showHeader: false)`.
- **Sections** (`activity_feed_sections.dart`): "This week" (Monday-start), then "May 2026" style month-year, then "Earlier" for rows with no timestamp.
- One card per section: `ground`, radius 24, no shadow on mobile, padding `(16,24,16,24)`.
  - Title: 24h `labelLarge` w600 `text.secondary`.
  - Rows 12 apart. Cards 24 apart.
- **Row anatomy** (`ActivityFeedRow`, 44h, padding h4, radius 12):
  - Left: 40 disc (`neutralSubtleOpacity`) with an 18 icon (`icon.regular`), 8 gap.
  - Middle column:
    - Title (`labelLarge` `text.accent`): "Received" / "Sent" / "Shielded" / "Sending..." / "Receiving..." / "Send failed".
    - 4 gap, then a subtitle row: pool icon 16 (crimson `shieldKeyholeOutline` or muted transparent) and "Shielded" / "Transparent" / "Mixed" (`labelLarge` w400 secondary).
  - Right column (max width 128, end-aligned):
    - Amount (`labelLarge` w600): `"+1.25 ZEC"` in green for inbound, `text.accent` for outbound.
    - 4 gap, then `"May 29, 13:40"` in `text.muted`, or "Failed" with a `skull` 12 in destructive.
- Row icons: received → `arrowDownCircle`, sent → `plane`, shielded → `shieldKeyholeOutline`, pending → `loader`.
- Tapping a row pushes `/activity/tx/<txid>?kind=`.
- New since `ff02152` (`activity_row_mapper.dart`): pool label "Ironwood" beside Shielded / Transparent / Mixed; migration rows "Migrating to Ironwood..." / "Migrated to Ironwood" / "Migration failed" (subtitle "Orchard → Ironwood", icon `migrationFast`); failed rows show "Refunded"; gift-card rows "Created a gift card" / "Redeemed a gift card" / "Creating a card..." / "Created N gift cards" / "Gift card creation failed" (detail: `activity/widgets/gift_card_activity_detail_view.dart`, fee row "Card fee").
- Txids are displayed in explorer byte order (`zcashDisplayTxidHex`, PR #750). The explorer link uses the configurable explorer (default CipherScan, 12.6). Received transactions no longer show a fee; fees are computed locally (PR #777).

**`MobileTransactionStatusScreen`** uses the same review-row and detail-card vocabulary as the send status screen:
- Received: **From** → arrow → Amount.
- Self-shield: Amount ("From transparent balance") → To "Shielded balance".
- Includes the expandable memo row and the fee row.

### 4.8 Settings (`MobileSettingsScreen`)

- `MobileTopNav.back(title: 'Settings')` with no back button.
- `ListView`, padding `LTRB(16, 12, 16, 112)`, with group cards 24 apart (955 lines now; two groups at `ff02152`, five plus a privacy card now).
- Group card: `MobileSurfaceCard(cornerRadius 24, padding (16,32,16,32))`. Group title in `labelLarge` w400 `text.secondary` (padding left 4 / bottom 8). `MobileListRow` 44h rows with a leading icon 20 in `icon.muted`.

| Group | Row (icon) → action |
|---|---|
| Personal (new) | "Coinholder voting" (vote) → `/voting` |
| | "My gift cards" (giftCard, with a "New" `labelBadge`) → `/payment-links` |
| | "Address book" (users) → `/settings/address-book` (was "Contacts" under Account) |
| Account | "Secret Passphrase" (key) → `/settings/seed-phrase` (disabled for hardware accounts) |
| | **"Viewing Key"** (eye, new) → `/settings/viewing-key`: passcode gate "Enter Passcode" / "Confirm your access", then "Full Viewing Key" inside `SensitivePrivacyOverlay`, "Copy viewing key" (toast "Viewing key copied"). Copy in `features/settings/viewing_key_copy.dart` (12.6). |
| | "Password" (lock) → `/settings/change-password` → toast "Passcode updated" |
| | "Profile Picture" (user) → picker sheet; value shows a 24 avatar + name |
| | "Account Name" (scroll) → edit sheet |
| Syncing (new) | "Keep screen awake" toggle (day icon), note "Prevents your phone from sleeping so sync can finish faster. The app still locks after 1 minute of inactivity." |
| System | "Endpoint" (endpoint), value host:port → `/settings/endpoint` |
| | **"Explorer"** (globe, new), value "CipherScan" or the custom host → `/settings/explorer` ("Current: …", option cards incl. "Custom" / "Any explorer you prefer", field "Explorer URL", "Update explorer") |
| | "Theme" (theme), value "System" / "Light" / "Dark" → theme sheet (option cards "System (Auto)", "Light", "Dark"; "Update" / "Cancel") |
| | "Face ID" / "Touch ID" / "Fingerprint" (lock), value "On" / "Off" (only if supported) |
| Privacy (new, `settings/widgets/mobile/mobile_network_privacy_card.dart`) | "Use Tor" toggle with statuses Off / Connecting… / Connected / Switching… / Failed / "Setting not saved" / "Switch failed", actions "Try again" / "Try direct connection" (6.5) |
| | "Private queries" On / Off (PIR transaction-detail recovery, "Experimental…", 12.6) |

A version footer (wordmark | "v{version}") ends the list. Donation is desktop-only (`/donation`).

**Seed phrase screen**
- Passcode gate first: "Enter Passcode" / "Confirm your access".
- Then, inside `SensitivePrivacyOverlay`:
  - Card (radius 32) with a `Wrap` of 90×32 word cells (`codeSmall` index "01" + `labelMedium` word).
  - "Copy" chip (inverse stadium) → toast "Secret passphrase copied".
  - Birthday card with copy icons.
- An iOS screenshot triggers a warning sheet: "Don’t take screenshots of your Secret Passphrase" / "I understand".

### 4.9 Onboarding (create flow)

**Flow:** Welcome → Method (step 2) → Intro (3) → Address types (4) → Things to know (5) → Secret passphrase (6) → Create passcode (7) → **Customise account (8, new)** → Biometrics (1.0) → `/home`. Progress is `step / 9` (was `/ 8`).

**Customise account** (new, `onboarding/mobile/mobile_customise_account_screen.dart`, PRs #336/#337/#485): "Customise Account" / "Add personality to your account by setting an account name and choosing a profile picture.", fields "Account name" and "Change profile picture", buttons "Continue" / "Creating wallet...". Default names come from `onboarding/create/account_persona_generator.dart` (adjective + archetype, e.g. "Steadfast Wayfinder"). Also used after Keystone, Ledger and import onboarding.

**`MobileOnboardingStepScaffold(progress, title, subtitle, child, bottomArea, aboveTitle?)`**
- `MobileTopNav.steps(progress)`.
- Scroll area, padding `(8,24,8,24)`:
  - Optional `aboveTitle` + 24.
  - Title in `displayLarge` (Young Serif 40), centred, `text.accent`.
  - 16 gap, subtitle in `bodyMediumStrong` `text.primary`, max width 320, centred.
  - 24 gap, child.
- Pinned bottom area, padding `(16,12,16,12)`.

**Welcome**
- Full-bleed hero illustration, fading to transparent at the top.
- Wordmark 106×40 at top 35.
- 38 gap, "Private Money.\nBy default" in Young Serif 48.
- 29 gap, primary "Get started ›" (min width 200).

**Method selection**
- "Welcome to Vizor" / "Select the method you want."
- 120h cards (radius 24, 1.5 `border.subtle`), 16 apart, each with art 180×120 on the right (now `method_*_card_bg.png` backgrounds).
  - Content padding 16: icon 20 at the top, label in `headlineMedium` (Young Serif 28) at the bottom.
  - "Create wallet" is emphasised with the dark `homeCard` fill; the others use `raised`.
  - Labels now: "Create Wallet", "Import Wallet", **"Link Vizor Desktop"** (new), "Connect Keystone", **"Connect Ledger"** (new, feature-gated `showLedger`). At `ff02152`: "Create wallet", "Import wallet", "Connect Keystone".

**Info steps**
- Dark `homeCard` info cards (radius 24, pattern overlay) or `ground` cards (radius 24, padding h20 / v44).
- Section headers: icon 24, 8 gap, `bodyLarge`.
- Copy examples:
  - "The Shielded World" / "Zcash (ZEC) built around financial\nprivacy & self-custody."
  - Primary "Tell me how Zcash works ›", plus a text link "I know how to use Zcash" (skip).

**Secret passphrase**
- Before reveal: a warning card (`homeCard`, radius 32, min height 440) with a 32 crimson key tile, "You are about to see your\nSecret Passphrase.", and body at 70% opacity.
- After reveal: `SeedCard`, a 3-column grid of "01 word" (`codeSmall` @ 45% + `bodyMedium`), 19 row gap.
  - "Copy" / "Copied" uses the sensitive clipboard and a haptic.
- Buttons: "Reveal phrase" → "Continue" → "Creating wallet...".

**Passcode**
- "Create Passcode" / "Confirm Passcode" (`displayLarge`), subtitle "6 digits length" / "Re-enter your passcode.".
- **Dots:** 6 diamonds, each a 14×14 square rotated 45° with radius 3, 8px horizontal padding. Filled = `icon.brandCrimson`; empty = `neutralSubtleOpacity`. The error text is positioned 36px below the dots' centre, outside the layout, so nothing shifts.
- **Keypad:** 320 wide `Wrap`, spacing 16.
  - 80×80 circles in `neutralSubtleOpacity`, digits in Young Serif 40.
  - Last row: [help | 0 | backspace].
  - Haptics: `digit()` on digits, `auxiliaryKey()` on delete.
  - Mismatch error: "Passcodes didn't match. Try again."
- The passcode **is** the wallet password on mobile (min length 6, per `password_policy.dart`).

**Biometrics**
- Hero art, then "Unlock your wallet\nwith Face ID" / "This is an easy and fast way to sign in.\nYou can switch back to passcode anytime."
- Primary "Enable Face ID" (faceId icon), ghost "Not now".

**Unlock**
- "Welcome Back" / "Enter your passcode to open Vizor", then dots and numpad.
- `PasscodeBiometricButton` ("Sign in with Face ID"): ghost pill, 36h.
- Help key → "Forgot Passcode?" sheet → "Are you sure?" sheet with a countdown destructive "Reset Vizor".

**Import**
- "Import Wallet": 24 slots in a dark card; "Paste secret phrase" / "Enter manually".
- Manual word entry: word field in `displayLarge` with suggestion chips.
- Birthday: date or block height tabs; "I don’t remember".
- New order: phrase → **Review Import** ("Review Import" / "Review your secret passphrase\nbefore import starts.", "Clear secret passphrase", "Confirm & continue") → birthday → passcode → customise. Progress `step / 6`.
- A BIP39 passphrase ("25th word", PR #461) can be entered only on desktop (`import/import_secret_passphrase_screen.dart`, "BIP39 Passphrase (Optional)"); on mobile it arrives only through the wallet link.

**Wallet link** (new, `onboarding/mobile/mobile_wallet_link_screens.dart`, 12.6): "Link with Desktop" / "Copy your desktop wallet to this phone"; steps "Open & unlock your Vizor desktop app", "Go to Settings → Link Vizor Mobile", scan; then "Select account" ("Link N accounts") and "Import contacts".

**Ledger** (new): connect, device sheet, birthday, customise (5.6).

### 4.10 Accounts

**Accounts sheet** (from the home top nav)
- Centred 56 avatar and name.
- "Other accounts": up to 4 rows of 48h.
  - Row: 40 avatar and name. A trailing copy icon copies the shielded address (toast "Address copied"). Tapping a row switches account.
- Bottom row: secondary "Manage accounts" (expanded) + an 80×50 primary `addNew` pill → `/add-account`.

**Accounts screen**
- Groups "Current" / "Other".
- Each row has a ⋯ menu: a dark inverse popover, radius 16, with "View secret phrase" (software accounts, new), "View viewing key" (new), "Copy address", "Send ZEC", "Edit account", divider, "Remove account" (in destructiveLight). Remove/reset copy now mentions migrations and unshared gift cards (a warning, no longer a block, PR #725).

### 4.11 `design_suggestion/` (repo root)

`index.html` is a Korean-language design-request memo (2026-06-12). It lists nine screens the developers built without Figma frames and asks the designer to formalize them:

1. Address book.
2. The Keystone signing flow (QR, then camera with progress %).
3. Swap review/detail.
4. The onboarding progress rule `step/(count+1)`.
5. Keystone badge placement.
6. The send-amount "validating" state.
7. The Face ID row and numpad glyph.
8. The birthday input: native calendar on iOS, sheet on Android; custom keypad removed.
9. Swap copy in sentence case.

The images are all iPhone 16 Pro, light theme, and **older than the code**:

| Image | Shows |
|---|---|
| `32_intro.png` | Intro step: progress track about 1/6, serif "The Shielded World", dark patterned card with a shield-Z icon, centred paragraph, dark pill "Tell me how Zcash works ›", skip link. |
| `35_passphrase_hidden.png` | Secret passphrase pre-reveal: large dark card with a crimson key tile, white heading, grey body, "Reveal phrase ›". |
| `31b_keystone_intro.png` | "Connect Keystone" / "Prepare your Keystone wallet": white card with "1. Check Keystone firmware" + link, divider, "2. Prepare to connect" with 4 numbered steps, "Continue ›". |
| `30f_address_book_empty.png` | Older "Address book" empty state: dark users tile, "No contacts yet", "⊕ Add contact"; floating tab bar with the crimson-tinted active pill. |
| `30g_address_book_add.png` | "Add contact" sheet over a scrim: knight avatar with pencil badge; Network / Name / Address fields; dark "Add contact" button; "Cancel". |
| `S0_address_book_list.png` | Contact row card with avatar, name + crimson Zcash badge, truncated t-address, trash icon; pinned "⊕ Add contact"; 4-tab bar. |
| `22_send_amount_ready.png` | "Enter amount": serif "0.25", "Max: 1.25 TAZ", "Sending to" + avatar + address, "Finish & Review", **custom grey keypad** (since replaced by the system keyboard). |
| `S1_swap_composer.png` | Swap composer (not relevant to Zafe). |
| `S2_keystone_sign_qr.png` | **Keystone QR sign screen:** black full screen, ✕ top right, "Confirm transaction" / "Use your Keystone wallet to scan this transaction QR code. Follow the steps on your device.", white rounded card with a square-module QR, bottom "Cancel" + dark outlined pill "Next step ›". The shield flow still uses this black layout; the send flow now uses the modal-card version (5.3). |

The `onboarding/` folder at the repo root is Korean developer docs (architecture, FFI, sync engine, Keystone, gotchas). It has no UI content.

`design_suggestion/` is unchanged at `4bff2e7`. New at the repo root: `FIGMA-AI-FIX.md` + `figma-ai-fix/` (agent instructions for editing Vizor's Figma design system, "Vizor Design System AI Test"), `docs/` (feature design notes: Gift Cards, Ledger, voting, resubmission, transparent recovery, password input source), `CONTRIBUTING.md`, `SECURITY.md`, `dev/` (payment-URI demo).

---

## 5. Hardware / external-signer UX (the analogue of Zafe's approval flow)

Since `ff02152` Vizor has two hardware signers: **Keystone** (QR, air-gapped) and **Ledger** (USB on desktop, Bluetooth on mobile; PRs #695–#744). Both are watch-only accounts in the wallet DB. `AccountInfo.hardwareSignerKind` (`lib/src/providers/account_models.dart`, `enum HardwareSignerKind {keystone, ledger}`; read via `accountProvider.notifier.hardwareSignerKindForAccount(uuid)`) replaced the old boolean-only gate (`isHardware` still exists). The send review (`mobile_send_screen.dart`) routes: Ledger → push `/send/ledger-sign` (returns `LedgerBroadcastArgs`), Keystone → push `/send/keystone-sign` (returns `KeystoneBroadcastArgs`), otherwise the software path. A `null` result runs `_recoverCancelledProposal` (`discardSendProposal`, then requote; failure copy "Could not finish cancellation. Try again.").

### 5.1 Concept (Keystone)

A Keystone account holds a UFVK imported from the device, plus `seedFingerprint` and `zip32Index`. Every spend goes through PCZT (ZIP 374). Non-TEX sends now use **Keystone batch signing** (PR #579): the device returns only signatures, not a signed PCZT.

```
proposeSend → createPcztFromProposal → preparePcztForKeystoneBatch (redact)
   → encodeZcashSignBatchUrParts ("ur:zcash-sign-batch/...")
   → [animated QR shown to device]
   ║ in parallel: addProofsToPczt(original)
   → scan "zcash-batch-sig-result" → decodeZcashBatchSignResponse (checks request id + per-message signature counts)
   → wait for proofs → storeAndBroadcastPcztsWithKeystoneSignaturesForProposal(proofs, signature blobs)
```

TEX sends keep the legacy full-PCZT path (`encodePcztUrParts` / `ur:zcash-pczt`, two transactions, "Confirm transaction 1 of 2" / "2 of 2"). **Proving still runs in the background while the device signs** (Ledger is the exception, 5.6).

### 5.2 Dart → Rust call sequence (`send/screens/mobile/mobile_keystone_sign_screen.dart`, `_preparePczt`, cached across rounds)

| # | Call (Dart binding) | Returns / notes |
|---|---|---|
| 0 | `rust_sync.proposeSend(...)` | `proposalId`, fee, `needsSaplingParams` |
| 1 | Sapling params check | If needed, `MobileSaplingParamsSheet`. Cancel throws `MobileKeystonePcztSigningAborted` and pops null. |
| 2 | `rust_sync.createPcztFromProposal(dbPath, lightwalletdUrl, network, proposalId, sendFlowId)` (now takes `lightwalletdUrl`); TEX: `createTexPcztsFromProposal` → `{pczts, signerPczts}` | PCZT bytes (consumes the proposal) |
| 3 | `buildKeystoneBatchSigningRequest(requestId: 'vizor-send-<sendFlowId>-transaction-N', pczts: [KeystoneBatchPcztSource(id: 'send-transaction-N', ...)])` → `rust_sync.preparePcztForKeystoneBatch` → `{redactedPczt, expectedSignatureCount}` | Service: `lib/src/features/keystone/services/keystone_batch_signing.dart`. Rust cap `ZCASH_SIGN_BATCH_MAX_SIGNATURES = 96` per transaction (`rust/src/wallet/sync/pczt.rs`). |
| 4 | `rust_keystone.encodeZcashSignBatchUrParts(requestId, messages, maxFragmentLen 140)` | `ur:zcash-sign-batch/...` fountain parts |
| 5 | `rust_sync.addProofsToPczt(pczt, spendParamsPath?, outputParamsPath?)` | **Started without await**, stored as the Future `MobileKeystonePcztSigningPayload.pcztWithProofs`. |
| 6 | `rust_keystone.resetUrSession()`, then `decodeUrPart(part, expectedUrType: 'zcash-batch-sig-result')` per frame | `UrDecodeResult{complete, progress, data?, urType?}` |
| 7 | `KeystoneBatchSigningRequest.decodeResponse` → `rust_keystone.decodeZcashBatchSignResponse(cbor, expectedRequestId, messageIds)` → `encodeKeystoneActionSigs` | Compact blob; each signature is `{pool 0=Orchard / 1=Ironwood, action index, 64-byte sig}` |
| 8 | Poll the proofs future every 200 ms | |
| 9 | Pop `KeystoneBroadcastArgs` (now lists: `pcztWithProofs`, `pcztWithSignatures`). The status screen (`send/services/send_flow.dart` `runSendBroadcast`) calls `rust_sync.storeAndBroadcastPcztsWithKeystoneSignaturesForProposal(...)` (TEX: `storeAndBroadcastSignedPcztsForProposal`) | Validates every signature against `rk` and every transaction **before any network I/O**, broadcasts in dependency order, then atomically stores only the accepted-or-ambiguous prefix (still broadcast-before-store). Rust owns proposal-lock cleanup. Statuses: `broadcasted`, `expired`, `broadcast_unknown`, `partial_broadcast`, `broadcasted_storage_failed`. |

- **Cancel:** the pushed route pops `null`; `dispose()` also calls `discardSendProposal` unless ownership was handed off. While cancelling, the title is "Cancelling…" and input is blocked (`AbsorbPointer`).
- Broadcast copy: "The transaction reached the network, but local tracking failed. Check Activity or an explorer before sending again."; for TEX: "The first transaction is stored locally and may have reached the network, but confirmation timed out. Check Activity before sending again." and "The first transaction was accepted, but the dependent transaction did not complete. Check Activity before sending again."
- New prepare errors: "This transaction uses too many inputs for Keystone batch signing. Try a smaller amount." / "This transaction uses inputs that Keystone batch signing cannot sign." (plus the old "Transaction expired before it could be signed.", "Required proving parameters could not be prepared.", "Keystone signing could not be prepared. Go back and try again.").
- Other bindings (`lib/src/rust/api/keystone.dart`): `encodePcztToUr`, `decodeUrToPczt`, `decodeAccountsFromCbor`, `decodeAccountsUr`. UR errors unchanged ("Invalid UR: missing type prefix", "Unexpected UR type: got …, expected …", "UR session reset: …").
- Crates: `ur` (KeystoneHQ/ur-rs), `ur-registry` now from git `valargroup/keystone-sdk-rust`, and `zakura-pczt` (section 7).
- Batch signing is reused by swap, Gift Cards, voting and the Ironwood migration (`swap/providers/swap_hardware_signing_service.dart`, `payment_links/services/payment_link_hardware_signing_service.dart`, `migration/.../keystone_signing.dart`).

### 5.3 Signing state machine and screens (`keystone/widgets/mobile_keystone_pczt_signing_flow.dart`)

`MobileKeystonePcztSigningFlow`: states unchanged, `_SignStage {preparing, showQr, scanning, failed}` plus `_decoding`, `_proofsFailed`, `_scanProgress`, `_scanHint`.

```
preparing ──ok──▶ showQr ──"Next step"──▶ scanning ──complete──▶ (decoding) ──▶ onSigned → pop(result)
    │                ▲                        │  bad frame → hint, stay; CBOR fail → reset UR session
    │                └──"Show transaction QR"─┘
    └──error──▶ failed ◀──── onSigned/proofs error (camera stopped)
Cancel = pop(null); blocked while _decoding
```

**Layout changed** (PR #305 "Refresh mobile Keystone signing flow"): the bottom `MobileModalCard` of `ff02152` is now a **full-screen two-step flow** with a top progress track (0.5 → 1.0).

- **Step 1** (QR): title "Step 1/2", subtitle "Scan with Keystone" (on failure: the caller's `failedTitle`, subtitle "Try again with Keystone").
  - Instruction "Tap [icon] on your Keystone, then scan this QR code"; while preparing "Loading QR code ...".
  - Primary **"Next step"** (was "Get signature"), then "Cancel". Optional auxiliary action, `signingContextLabel` badge and `requestDetails` widget.
  - QR (`keystone_pczt_qr_stage.dart`): frame interval fixed at **200 ms** (`_keystoneQrFrameInterval`; was 100 ms), square modules, EC M, configurable quiet zone (default 3), on white.
- **Step 2** (scanner): "Step 2/2" / "Confirm with Keystone", 285 px scan window.
  - Caption from `scanCaption` (send passes "Scan the QR code on your Keystone to finish sending"; default "Scan the QR code on your Keystone to confirm"). Progress "Scanning... N%" (held 360 ms at 100%), then "Reading signature...", then optional `finalizingSignatureLabel`.
  - Torch button (semantics "Toggle flashlight") and a new **"Show transaction QR"** button that returns to step 1 and resets the UR session (`_scanSessionResetToken++`). The send flow now has the way back that only the shield flow had at `ff02152`.
  - Hints unchanged: "Open the signed transaction QR on Keystone, then scan again." (overridable, `unexpectedUrMessage`), "Keep the QR code steady and fully visible.", "This QR code could not be decoded as a Keystone signature."
  - Camera errors: "Camera access is off. Allow it in Settings to scan Keystone signatures." / "The camera is unavailable right now." (iOS now distinguishes restricted from denied, PR #761).
  - `recoverSignedCallbackErrorInScanner`: an `onSigned` error becomes a scanner hint instead of the failed state.
- `keystone/widgets/keystone_scan_help_overlay.dart` (`KeystoneScanHelpOverlay`): "Scanning issues?" tooltip, "Update to the latest Keystone firmware at keyst.one/firmware". Used on desktop and in migration, not in mobile send.

### 5.4 Keystone shield screen (`home/screens/mobile/mobile_keystone_shield_screen.dart`)

Essentially unchanged: still the older full-screen **black** layout with `_ShieldSignStage {preparing, showQr, scanning, broadcasting, failed, broadcastWarning}` and the legacy `redactPcztForSigner` + `encodePcztUrParts` + `extractAndBroadcastPczt` pipeline (not batch signing).
- Title "Shield transparent balance" / "Broadcasting shield tx"; subtitle "Use your Keystone wallet to scan this shielding QR code. Follow the steps on your device." / "Keep Vizor open while the transaction is sent."
- Bottom bar: ghost "Cancel", primary "Next step ›" ("Broadcasting..."; secondary "Show QR" while scanning); terminal "Back to wallet".
- Changes: `createShieldTransparentPczt` now takes `lightwalletdUrl`; the screen mixes in `PaymentUriBusySurfaceHoldMixin` (an incoming payment link waits until it closes); QR at the global 200 ms cadence.
- Uncertain broadcast: "The shield transaction status is uncertain. Check activity before trying again."

### 5.5 Keystone onboarding (`onboarding/mobile/mobile_keystone_screens.dart`)

Copy and structure are unchanged from `ff02152`: "Connect Keystone" intro (two cards: "1. Check Keystone firmware" with the keyst.one/firmware link; "2. Prepare to connect"), "Scan QR Code" (UR `zcash-accounts`, "Scan the Keystone account QR" / "Reading accounts..."), "Select account" ("N accounts found", radio cards with UFVK 12 + " ... " + 10, CTA "Select account"), then birthday (reuses `MobileImportBirthdayScreen`). Progress 0.2 / 0.4 / 0.6 / 0.8.

One change: when a passcode already exists, the birthday step no longer imports directly. It pushes `/onboarding/customise-account` with `CustomiseAccountArgs(setupArgs: SetPasswordScreenArgs.importKeystone(...))`, the new name/avatar step (`onboarding/mobile/mobile_customise_account_screen.dart`, 4.9). Without a passcode it still goes to `/onboarding/set-passcode`.

### 5.6 Ledger (new)

**Send** (`send/screens/mobile/mobile_ledger_send_sign_screen.dart`): create base PCZTs → `redactPcztForSigner` → **`addProofsToPczt` awaited before any device interaction** (sequential, unlike Keystone: the device is attended) → per round, phase `awaitingDevice` and `ledgerPcztSignerProvider(account, redacted)` → phase `saving` → `_checkpoint`.
- `_checkpoint` calls `ledgerSignedOperationService.checkpoint` / `checkpointBatch` (Rust `ledger_checkpoint_signed_operation[_batch]`), which **durably stores the signed transaction before any broadcast**. The status screen then broadcasts (`ledger_broadcast_signed_operation`) and acknowledges (`ledger_ack_signed_operation`). Signed PCZT bytes never cross FRB.
- Each round's signature is kept, so a TEX send that fails on its second approval retries only the second one.
- Failure copy: "Transaction expired" / "New transaction required" / "This transaction can no longer be signed. Create and review a new transaction." / "Create new transaction"; "Ledger needs attention" / "Action needed"; "Ledger signing failed" / "The transaction was rejected on your Ledger."; checkpoint failure "Could not save signed transaction" / "Signature preserved" / "Your Ledger signature is preserved. Retry saving without approving another transaction." / "Retry saving"; terminal "Signed transaction needs attention" / "Recovery required" / "Vizor could not verify the saved transaction. Do not sign or send it again."

**Stages** (`features/ledger/services/ledger_signing_progress.dart`; rules in `docs/ledger/signing-phase-guidance.md`): `enum LedgerSigningStage {preparing, sending, reviewing, finishing}`.

| Stage | Title | Body | Status |
|---|---|---|---|
| preparing | "Preparing transaction" | "Please wait while Vizor prepares your transaction." | "Please wait" |
| sending | "Processing with Ledger" | "Your Ledger is preparing to sign. It may seem unresponsive for about 10 seconds. Keep your Ledger connected." (30 seconds for older/unknown models, `ledgerHasShortSigningWait`: stax/flex/europa) | "Preparing to sign" |
| reviewing | "Check your Ledger" | "Review and approve when prompted on your Ledger." | "Review on device" |
| finishing | "Finishing transaction" | "Keep Vizor open." | "Please wait" |

- The design rules: stages come only from real protocol boundaries, with **no timers, percentages or ETAs**; stages only move forward, are keyed to an attempt generation and ignore late events after cancel/retry; "when prompted" avoids claiming the device prompt is already visible.
- Modal: `features/ledger/widgets/ledger_signing_modal.dart`, `LedgerSigningModalPhase {preparing, awaitingDevice, saving, broadcasting, failed}`. Mobile reviewing text "Confirm on your Ledger" / "Waiting for your approval"; rounds "Transaction N of M"; readiness "Checking your Ledger" / "Vizor is checking whether the Zcash app is ready.", "Confirm opening Zcash" / "Confirm the request on your Ledger. Vizor will reconnect automatically."; persistent "Open the Zcash app" / "Keep it open on your Ledger."

**Errors by stable code** (PR #724, `features/ledger/ledger_error_codes.dart`): `classifyLedgerError` parses Rust prefixes (`ledger_status_xxxx:`, `ledger_cancelled:`, `ledger_capacity:`, `ledger_signature_mismatch:`, `ledger_linux_usb_access`) into `LedgerFailureKind` (userRejected 0x6985/0x5501, deviceLocked, pinNotSet, appNotInstalled, deviceBusy, appWrongState, hostRequestRejected, wrongApp 0x6e00/0x6d00, capacityExceeded 0x6a84, deviceInternalError, unknownStatus, cancelled, signatureMismatch, usbPermission, transportLost, appUpdateRequired, saplingUnsupported, other). `ledger_failure_guidance.dart` maps kinds to title/body ("Request declined" / "Declined on your Ledger. Try again when ready."; "Wrong Ledger" / "Connect the Ledger that holds this account."; "Request too large" / "Too many inputs for your Ledger. Try a smaller amount.") and records whether retrying the same request is pointless. Still string prefixes, not an FRB enum.

**Operation gate and recovery**
- `ledger/services/ledger_operation_lifecycle.dart`: `LedgerOperationLifecycle.run` (zone-scoped lease; `quiesceAndDrain` / `resume` before wallet deletion; "Ledger operations are paused for wallet changes."), `LedgerOperationClaimRegistry.tryClaim(operationId)` so background recovery never consumes an operation an open signing surface owns.
- `ledger/services/ledger_operation_recovery.dart`: `LedgerOperationRecoveryCoordinator` replays the outbox (Rust table `vizor_ledger_signed_operations`, `rust/src/wallet/ledger/operations.rs`) in two states, `signed_pending_broadcast` → broadcast again, `result_pending_ack` → handle result and ack. A checkpoint is dropped once wallet history owns the transaction.
- `ledger/services/ledger_signing_status_gate.dart` serializes mobile signing and waits out the device's post-signing screen.
- **Input reservations** (`rust/src/wallet/sync/proposal_locks.rs`, applies to all hardware sends): table `vizor_send_proposal_locks` with phases `session` / `signed` / `broadcast` / `release` / `legacy`, a session id and `retain_until_expiry`. At startup `recover_before_balance` / `recover_previous_process` unlock inputs held by earlier sessions unless retained; `checkpoint_owner` moves a lock to `signed` in the same SQLite transaction as the outbox row; an ambiguous broadcast calls `retainSendProposalLockUntilExpiry`. Normal exit runs `shutdown_signing_reservations` from `core/lifecycle/signing_shutdown_host.dart`. Built on `zcash_client_backend`'s `OutputLockStore::lock_outputs` API (`data_api/locking.rs`).
- Single change output for Ledger (`rust/src/wallet/sync/send.rs` `zip317_helper(is_ledger)`: "The Ledger app rejects a second shielded change output during PCZT validation"); software/Keystone change is split into outputs of at least 0.1 ZEC. Ledger limits: 32 shielded actions per pool, 32 transparent inputs per shield round.

**Other Ledger surfaces:** `home/widgets/ledger_shield_signing_overlay.dart` (consecutive shield rounds; "Round N was sent. The remaining funds are below the shielding threshold."); onboarding `onboarding/mobile/mobile_ledger_{connect,birthday,device_sheet}.dart` ("Connect Ledger" / "Select your Ledger, then approve sharing its viewing key to add a watch-only account." / "Approve on Ledger"); `docs/ledger/connection-and-shielding.md` ("Cancelling the host does not promise dismissal of the device's approval prompt"). Rust: `rust/src/api/ledger.rs` (`ledger_sign_pczt`, `ledger_sign_with_progress` stream of `LedgerSigningEvent`, mobile APDU plans `ledger_build_pczt_signing_apdu_plan` / `ledger_finalize_mobile_pczt_signing`, `ledger_export_account`, `ledger_cancel_operation`, `ledger_validate_supported_pczt`), `rust/src/wallet/ledger/{apdu,transport,operations,parse,serializer}.rs`. Native: `ios/Runner/LedgerMobileHandler.swift`, `android/.../LedgerMobileHandler.kt` (Ledger DMK; Android `minSdk 30`).

**Shared hardware plumbing:** `swap/providers/swap_hardware_signing_service.dart` (draft PCZT → encode/decode → `addProofsForSigning` → `broadcastSignedPczt`, which takes ownership of the proposal lock: release on a definite result, retain until expiry on an ambiguous one); `swap/models/swap_hardware_broadcast_result.dart` (replaces `swap_keystone_broadcast_result.dart`). Automatic rebroadcast of unmined transactions is described in `docs/transaction-resubmission.md`: a transaction mined earlier is never resubmitted during a rewind (`vizor_mined_transactions` trigger, tip height and hash must be unchanged); expired transactions are not rebuilt.

**Worth borrowing for Zafe:** the signatures-only response checked for request id and count (maps to FROST shares plus the leader's proven PCZT); checkpointing signatures durably before broadcast with a two-state outbox and a claim registry; session-scoped input locks released at startup and retained until expiry on ambiguous broadcasts; four honest stages without timers; errors as stable codes with a "retry is pointless" flag; "Signature preserved, retry saving".

---

## 6. State and data flow in Dart

### 6.1 Startup (`lib/main.dart`, `lib/app.dart`, `lib/src/app_bootstrap.dart`)

```
main(): assert(debugCheckFormFactorMatchesPlatform()); runZcashWalletApp();
initializeZcashWalletRuntime(): WidgetsFlutterBinding.ensureInitialized();
    SecureStorageDiagnostics.instance.initialize(); await RustLib.init(); (voting observability logging)
    initializeNetworkPrivacyRuntime();   // Tor route before any request; never waits for bootstrap (#602)
    (fast-testnet-migration / regtest Ironwood activation height); (desktop window init)
bootstrap = await loadAppBootstrap();
runApp(BootstrappedZcashWalletApp) → ProviderScope(key: ValueKey(generation), overrides: [
    appBootstrapProvider.overrideWithValue(bootstrap),
    appBootstrapRetryProvider.overrideWithValue(reload)])   // retry bumps key → fresh container
// whole app wrapped in SigningShutdownHost (core/lifecycle/signing_shutdown_host.dart): on exit
// appShutdownSignal.begin(), cancelFullSync, stopMempoolObserver, shutdownSigningReservations.
// Linux adds LinuxKeyringStartupHost (core/widgets/linux_keyring_gate.dart).
// buildProductionZcashWalletApp applies the enhance-PIR policy (setEnhancePirEnabled) after bootstrap.
```

**`loadAppBootstrap()`** does, in order:
1. Ensures the wallet DB name.
2. Recovers any interrupted password rotation.
3. Reads settings from secure storage: network, RPC endpoint, theme, privacy mode, biometric flag, password configured.
4. Sets `isUnlocked = hasSessionPassword`.
5. Calls `rust_wallet.walletExists`. If the wallet exists, calls `ensureWalletDbMigrated`.
6. Calls `listAccounts` and merges the result with the secure-store `zcash_accounts` JSON. Rust is authoritative for existence; Dart owns the name, order, `profilePictureId` and `isHardware`.
7. Resolves the active account.
8. If unlocked, preloads the sync snapshot: `getSyncStatus`, `getBalance`, `getTransactionHistory(limit: 10)`.
9. Computes `initialLocation`: `/welcome`, `/unlock` or `/home`.

Added since `ff02152`: `ensureIosSecureStoreAccessibilityMigrated()` runs first (6.4); bootstrap also reads the explorer URL template, sync keep-awake enabled / prompt-seen, and the enhance-PIR preference; it backfills legacy hardware accounts and resolves each account's `hardwareSignerKind`. `core/layout/app_process_work_policy.dart` `canRunAppProcessWork({isInForeground, formFactor, shutdownSignal})` says Dart background work runs only in the foreground on mobile (always on desktop).

Failures produce `AppBootstrapState.blocked(kind)`, where `kind` is `secureStorageUnavailable | startupFailure | walletDbMigrationFailed`. The router then shows `/storage-unavailable` (`StorageUnavailableScreen`) with a retry. Example copy: "Vizor needs access to secure storage before it can open your wallet."

**`ZcashWalletApp`** is `MaterialApp.router(theme: buildLegacyLightTheme(), darkTheme: ..., themeMode: ref.watch(themeModeProvider), routerConfig: router, builder: ...)`. The `builder` wraps the child as `AppThemeHost(themeMode) → … → NetworkFallbackToastHost → GestureDetector(unfocus on outside tap)`.

### 6.2 Riverpod pattern

**Riverpod setup**
- `flutter_riverpod ^3.3.1`.
- All providers are hand-written: no codegen, no `@riverpod`, no `family`.
- All providers are global and non-autoDispose, except price data, which is `.autoDispose`.
- Most notifiers seed `build()` from `ref.watch(appBootstrapProvider)`, so a bootstrap retry resets everything.

```dart
// lib/src/providers/privacy_mode_provider.dart (complete, representative)
class PrivacyModeNotifier extends Notifier<bool> {
  static final _store = AppSecureStore.instance;
  @override
  bool build() => ref.watch(appBootstrapProvider).privacyModeEnabled;
  Future<void> set(bool enabled) async {
    await _store.writePlain(kPrivacyModeEnabledKey, enabled ? 'true' : 'false');
    state = enabled;
  }
  Future<void> toggle() => set(!state);
}
final privacyModeProvider =
    NotifierProvider<PrivacyModeNotifier, bool>(PrivacyModeNotifier.new);
```

| Provider | Type | Role |
|---|---|---|
| `walletProvider` | `AsyncNotifierProvider<WalletNotifier, WalletState>` | Derived: `hasWallet`, `unifiedAddress`, `activeAccountUuid` |
| `accountProvider` | `AsyncNotifierProvider<AccountNotifier, AccountState>` | Create, import, import Keystone, switch, rename, remove, reset, mnemonic access; persists `zcash_accounts` / `zcash_active_account` |
| `syncProvider` | `AsyncNotifierProvider<SyncNotifier, SyncState>` | Full sync over an FRB `Stream` (see below), mempool observer, 10s poll, lifecycle hooks |
| `appSecurityProvider` | `NotifierProvider<AppSecurityNotifier, AppSecurityState{isPasswordConfigured, isUnlocked}>` | Two-phase `preparePasswordSetup` / `commitPasswordSetup` / `rollbackPasswordSetup` (so the router never sees half-done onboarding), `unlock`, `lock`, `changePassword` |
| `biometricUnlockProvider` | `AsyncNotifierProvider` | `enable(passcode)`, `disable`, `readPasscode(reason)` |
| `privacyModeProvider`, `themeModeProvider` | `NotifierProvider` | Persisted UI prefs |
| `receiveAddressServiceProvider` | `Provider<ReceiveAddressService>` | Retries a DB lock after 300ms, 1s and 2s, then "Wallet is busy. Try again in a moment." |
| `routerRefreshProvider` | `Provider<RouterRefreshController>` | `requestRefresh()`, `pauseWhile(action)` |
| `rpcEndpointProvider`, `rpcEndpointFailoverProvider`, `rpcEndpointLatencyProvider` | `NotifierProvider` | Endpoint selection, chain-name verification, automatic failover + toast |
| `zecHomeMarketDataProvider` etc. | `NotifierProvider.autoDispose` | CoinGecko price and 24h change |
| New since `ff02152` | | `networkPrivacyProvider` (6.5), `zcashExplorerProvider`, `enhancePirProvider`, `syncKeepAwakeProvider` family, `paymentRequestFlowProvider`, `paymentUriPrefillProvider`, `migrationSendGateProvider`, `chainUpgradeStatusProvider`, `syncDisplayPercentageProvider`, voting providers under `providers/voting/` |

**Sync pipeline**
- `rust_sync.startFullSync(dbPath, lightwalletdUrl, network, mode: 1)` returns a Dart `Stream<ApiSyncProgressEvent>`, fed by a Rust `StreamSink`.
- A generation counter `_syncGen` drops stale events after a restart.
- `runWithSyncPausedForAccountMutation(ref, action)` (`providers/wallet_mutation_guard.dart`) pauses sync around DB-mutating operations.

### 6.3 Calling Rust and mapping errors

**Call style**
- Providers and services call the generated bindings directly: `import '../rust/api/sync.dart' as rust_sync;` (likewise `rust_wallet`, `rust_keystone`, `rust_secret`).
- There are no isolates and no `compute()`. FRB runs normal functions on its thread pool and returns `Future`s.
- `#[frb(sync)]` is used for trivial getters and cancels (`set_sync_mode`, `cancel_full_sync`, `reset_ur_session`, …).

**Rust side**
- API functions return `Result<T, String>` and wrap their bodies in a `catch_unwind` helper, so panics become `"Rust panic: …"` errors.

**Dart side: error mapping is string matching**
- `onboarding_error_messages.dart` strips `Exception: ` and `AnyhowException(...)`, then maps known substrings to friendly copy.
- `providers/sync_failure.dart` has `classifySyncFailure(Object) → SyncFailure{kind: network|endpoint|databaseBusy|databaseFatal|chainRecovery|parseFatal|unknown, userMessage, showSettingsAction}`.
- Send has its own friendly-error mapper (4.6).
- `SyncFailureKind` now also has `torUnavailable` and `privateStatusCoverage`; there is a new `swap_failure_policy.dart`.

**What changed since `ff02152`:** most API functions still return `Result<T, String>` (about 86 in `sync`, 35 in `wallet`, 23 each in `ledger` and `keystone`), but two areas moved away from free-text matching:
- **Voting** returns typed errors: `voting_session.rs` returns `Result<_, VotingErrorView>` (from `zcash_voting::wire`: `kind: VotingErrorKindView, retryable, message, bundleIndex, snapshotHeight, requiredWeightZatoshi`). Streamed steps carry `ApiRoundStepError` because FRB drops a stream's `Result`. Dart wraps them in `lib/src/services/voting/voting_rust_exception.dart` (`VotingRustException`) and classifies by `kind`.
- **Ledger** uses stable string prefixes (`ledger_status_6985:`, `ledger_capacity:`, …) parsed by regex into `LedgerFailureKind` (5.6).

*For Zafe: prefer typed errors (an FRB-mirrored enum, as voting does) over substring matching; if a string must cross the bridge, use a stable code prefix as Ledger does.*

### 6.4 Secure storage and secrets (`core/storage/app_secure_store.dart`, singleton `AppSecureStore.instance`)

**Library and options**
- `flutter_secure_storage ^10.0.0`.
- Service name: `com.keplr.vizor.secure_store` on mainnet, `com.keplr.vizor.<net>.secure_store` on other networks.
- iOS: `IOSOptions(accountName: service, accessibility: KeychainAccessibility.first_unlock_this_device)` (was `first_unlock` at `ff02152`; PR #462). Existing items are migrated at bootstrap through `com.zcash.wallet/keychain_accessibility_migration` (`ensureFirstUnlockThisDeviceOnly`, native `ios/Runner/KeychainAccessibilityMigrator.swift`; staged under `.accessibility-migration-v1`, allow-listed services; failure throws `SecureStorageUnavailableException`).
- macOS: `MacOsOptions(usesDataProtectionKeychain: true)`; mnemonics use a separate service with `KeychainAccessibility.unlocked`.
- Windows: a fork in `third_party/flutter_secure_storage_windows` (`VIZOR_FORK.md`): serialized read-modify-write across processes, atomic file replacement, validated backup. Linux: `core/storage/linux_keyring_coordinator.dart` + `linux_secret_operation_guard.dart` (session generation, "Quit and restart Vizor before continuing."). `core/storage/secure_storage_diagnostics.dart` records evidence of missing keyring entries without keys, values or paths.
- Android: `AndroidOptions.defaultOptions` (mainnet), or `sharedPreferencesName: service` on other networks.

**Keys**

| Key | Content |
|---|---|
| `zcash_wallet_db_name` | `zcash_wallet_<24hex>.db` |
| `zcash_accounts` / `zcash_active_account` | account metadata JSON / active uuid |
| `zcash_wallet_network` | network |
| `zcash_theme_mode`, `zcash_privacy_mode_enabled`, `zcash_biometric_unlock_enabled` | plain prefs |
| `zcash_rpc_endpoint_url`, `zcash_rpc_endpoint_preset` | endpoint |
| `zcash_secure_store_salt` | salt for the encryption KDF |
| `zcash_password_verifier`, `zcash_password_verifier_salt` | PBKDF2-HMAC-SHA256 (100k iterations, 32 bytes) verifier, computed in Rust |
| `zcash_rotation_in_progress` | password-change journal |
| `zcash_account_mnemonic_<uuid>` | since PR #461 a versioned JSON envelope `SoftwareWalletSecret{mnemonic, bip39Passphrase}` (`core/security/software_wallet_secret.dart`, `version: 1`; a raw legacy string still decodes), AES-256-GCM encrypted with a PBKDF2 key derived from the passcode. Payload JSON: `{"v":1,"n":nonce,"c":ciphertext,"m":mac}`. Encrypted and decrypted by `rust_secret.encryptSecretPayload` / `decryptSecretPayload`. |

**Behaviour**
- The session password is held in memory only. `lock()` clears it.
- **The SQLite wallet DB is not encrypted.**
- The DB lives at `getApplicationSupportDirectory()/zcash_wallet_<hex>.db`. Sapling params live in `<appSupport>/sapling_params/`.

**Biometrics**
- There is **no `local_auth`**. A custom channel `com.zcash.wallet/biometric_unlock` escrows the passcode:
  - iOS: a keychain item with `.biometryCurrentSet`.
  - Android: an AndroidKeyStore AES-GCM key with `setUserAuthenticationRequired(true)` and `setInvalidatedByBiometricEnrollment(true)`, unlocked via `BiometricPrompt`. This requires `MainActivity` to be a `FlutterFragmentActivity`.
- `device_owner_auth` gates a wallet reset.

**Privacy**
- Android sets `FLAG_SECURE` through `com.zcash.wallet/privacy_shield`.
- iOS screenshot detection uses `EventChannel('com.zcash.wallet/screenshots')` (`core/platform/screenshot_observer.dart`).
- **Capture protection now covers iOS and macOS too** (PR #689): the same `privacy_shield` channel drives `SecureScreenshotShield` in `ios/Runner/AppDelegate.swift` (re-parents the window layer into a `secureTextEntry` canvas so screenshots and recordings are blank) and Mission Control capture suppression on macOS. Native reports `protectionStatusChanged` (applied / pending / failed / disabled). New `core/privacy/route_coverage_aware.dart`.
- Sensitive clipboard: `com.zcash.wallet/sensitive_clipboard`, now **iOS and Android** (`android/.../SensitiveClipboardHandler.kt`: `android.content.extra.IS_SENSITIVE` and auto-clear), 1 minute default expiry.
- Biometric labels distinguish Touch ID from Face ID (PR #645): `services/biometric_unlock.dart` `BiometricKind {face, touchId, fingerprint}`, icon via `core/widgets/biometric_icon.dart`. Still no `local_auth`.
- Desktop only: app-password input source snapshot/restore (`core/input/app_password_input_source.dart`, channel `password_input_source`, PR #763) and a "Caps Lock is on" tooltip (`core/input/caps_lock_monitor.dart`, `caps_lock_warning.dart`).

**Background work** (changed)
- **iOS background sync was removed**: `BackgroundSyncManager.swift`, `SyncProgressStreamHandler.swift`, `TxTrackManager.swift` and `RpcEndpointConfigStore.swift` are gone, and the old `com.keplr.vizor.sync` task id is cancelled at launch. Normal sync has one path: the foreground FRB stream.
- iOS `BGContinuedProcessingTask` / `BGProcessingTask` now serve only the Ironwood migration: watching preparation confirmations through a read-only C FFI (`rust/src/ffi.rs`, `rust/src/migration_preparation.rs`, native lightwalletd queries in `NativeLightwalletdClient.swift`, no scanning) and broadcasting pre-signed transactions from a durable outbox (`BackgroundMigrationOutbox*.swift`). Android uses a WorkManager worker over JNI (`rust/src/android_jni.rs`). Details in 12.4.
- **Keep awake while syncing** (mobile, PRs #322/#603): channel `com.zcash.wallet/screen_awake` (`services/native_screen_awake.dart`; Android `FLAG_KEEP_SCREEN_ON`, iOS idle timer), `providers/sync_keep_awake_provider.dart` (`SyncKeepAwakeNotifier`, `SyncKeepAwakePrivacyLockNotifier`, ETA estimate), hosts `core/widgets/mobile/sync_keep_awake_{native_host,privacy_lock_host,interaction_listener}.dart`, prefs `zcash_sync_keep_awake_enabled`, `zcash_sync_keep_awake_prompt_seen`. Copy: setting "Keep screen awake" / "Prevents your phone from sleeping so sync can finish faster. The app still locks after 1 minute of inactivity."; prompt (when the ETA exceeds 1 minute) "Stay awake to sync?" / "Your phone pauses syncing when screen is off. This allows sync to finish faster." / "The app locks after 1 minute of inactivity. Syncing continues behind the lock."; lock screen "Vizor is syncing", "Synced successfully. Unlock Vizor to continue.", "Sync paused. Unlock Vizor to continue.", "Unlock Vizor".

### 6.5 Network privacy: Tor (new since `ff02152`; desktop PRs #460–#472, mobile #482, #602, #628)

- **Implementation:** arti embedded in-process through `zakura-client-backend`'s `tor` feature (`zcash_client_backend::tor::{Client, DormantMode, Timeouts}`); no external tor binary. The data directory comes from `getTorDataDirectoryPath` and is excluded from iOS backups.
- **Policy** (`rust/src/network_privacy.rs`): process-wide statics `TOR_DESIRED`, `TOR_STATUS` (Direct / Bootstrapping / Ready / Failed), `TOR_CLIENT`. **Fail-closed:** the desired route flips before bootstrap, so requests wait instead of falling back to clearnet; in-flight direct I/O is tracked and drained when switching to Tor. `TOR_BOOTSTRAP_TIMEOUT` 3 minutes ("Tor could not connect. Check your internet connection and try again."). `RouteDecision {Direct, TorShared, TorIsolated}` via `tor_client_for_route(isolated, cancelled)`.
- **FRB API** (`rust/src/api/network_privacy.rs`): `begin_network_privacy_enable` / `fail_network_privacy_enable` (sync), `quiesce_network_privacy_direct_requests` (5 s), `configure_network_privacy(enabled, tor_directory) -> NetworkPrivacyStatus`, `get_network_privacy_status`, `is_tor_enabled`, `set_network_privacy_dormant`; Dart HTTP over Tor: `tor_http_get` / `tor_http_post` / `tor_http_begin_request` / `tor_http_cancel_request` / `tor_http_download` (30 s body timeout: "Tor HTTP request timed out"); desktop `start_tor_update_relay` / `stop_tor_update_relay`.
- **What is routed:** lightwalletd gRPC (`wallet/sync_engine/lwd.rs`; sync on the shared circuit, **broadcasts on an isolated circuit** via `open_isolated_lwd_channel`), PIR queries (isolated), voting transports, and all Dart HTTP through `lib/src/core/network/network_http_client.dart` (`RustTorHttpBridge`; `DirectNetworkRequestsBlockedException`, `NetworkHttpRequestCancelledException`): price, swap/NEAR Intents, wallet link, Sapling params, voting HTTP, desktop updates. The one exception is iOS background migration, which uses a pinned direct channel (`open_background_direct_lwd_channel`).
- **Dart** (`lib/src/providers/network_privacy_provider.dart`): preference `zcash_tor_enabled` in SharedPreferences; `NetworkPrivacyState{torEnabled, status: off|connecting|connected|failed, targetTorEnabled, softwareUpdatesAvailable, error, startupNotice}`. `initializeNetworkPrivacyRuntime()` runs before bootstrap and never waits for Tor. `startTorDormancyLifecycle` makes Tor dormant when the app is hidden on mobile. The saved route may be stricter than the enforced one, never laxer (`networkPrivacyPersistedRouteIsSafe`).
- **UI:** desktop `settings/widgets/network_privacy_control.dart` (also embedded in the pre-wallet custom endpoint panel), mobile `settings/widgets/mobile/mobile_network_privacy_card.dart`. Copy: "Use Tor"; off: "Tor is off. Requests to the Zcash network, in-app services, and software updates connect directly."; connected: "Vizor’s network requests go through Tor. Links opened in other apps use those apps’ network settings." (iOS adds "Ironwood private migration uses a direct connection while Vizor is closed."); failed: "Connection failed" / "Direct requests remain blocked. Try again or turn off Tor." with "Try direct connection"; switching off: "Switching to direct…", "Switch failed" / "Vizor could not switch to a direct connection. Try again."; startup: "Couldn't connect to Tor. Network requests are paused."
- **Status in the top nav:** "Connecting to Tor…" / "Tor couldn't connect..." (3.7).

---

## 7. flutter_rust_bridge setup

**`flutter_rust_bridge.yaml`** (entire file):

```yaml
rust_input: crate::api,zcash_voting::wire
rust_root: rust/
dart_output: lib/src/rust
```

**Codegen:** run **`scripts/generate-rust-bridge.sh`** (new; upstream AGENTS.md now points to it). Bare `flutter_rust_bridge_codegen generate` fails there: the script puts a Python `cargo` shim on `PATH` that rewrites `super let` to `let` in `cargo expand` output (from `std::pin::pin!`, which FRB 2.11.1's `syn` cannot parse); Rust still compiles the original code. The yaml is unchanged. FRB is pinned exactly on both sides: `flutter_rust_bridge = "=2.11.1"` in Rust and `flutter_rust_bridge: 2.11.1` in pubspec.

**Generated output**
- Dart: `lib/src/rust/frb_generated{,.io,.web}.dart` and `lib/src/rust/api/{gift_card_tracking,keystone,ledger,network_privacy,secret,simple,sync,voting,voting_session,wallet}.dart` (`voting_config.dart` is gone; `api/mod.rs` has `pub use crate::api::voting as voting_config;`), plus mirrored types.
- Rust: `rust/src/frb_generated.rs`.
- Dart startup must call `await RustLib.init()`.

**`rust/Cargo.toml`**
- Package `rust_lib_zcash_wallet`, `crate-type = ["cdylib", "staticlib", "rlib"]`. `staticlib` is used by iOS and `rlib` by tests.
- `[profile.dev] opt-level = 3`, because proving is unusable at opt-level 0.
- Key dependencies. **The librustzcash family is now the Zakura forks**, renamed with `package = "zakura-..."` while keeping the lib names, so `use orchard` / `use zcash_client_backend` still compile (at `ff02152`: `zcash_client_backend 0.23`, `zcash_client_sqlite 0.21`, `orchard 0.14`, `zcash_primitives 0.28`, `pczt 0.7`):
  - `zakura-client-backend =0.1.0-rc7 [orchard, transparent-inputs, lightwalletd-tonic, lightwalletd-tonic-tls-webpki-roots, tor, sync, pczt]`
  - `zakura-client-sqlite =0.1.0-rc7 [orchard, transparent-inputs, unstable, serde]`
  - `zakura-primitives`, `zakura-address`, `zakura-keys [orchard]`, `zakura-protocol`, `zakura-transparent`, `zakura-orchard`, `zakura-proofs [local-prover]`, `zakura-sapling-crypto`, `zakura-pasta-curves`, `zakura-jubjub`, `zakura-bls12-381`: all `=2.0.0`; `ff 0.14`, `shardtree 0.7`
  - `zakura-pczt =0.1.0-rc4 [orchard, io-finalizer, prover, signer, spend-finalizer, tx-extractor, zcp-builder]`
  - PIR: `zakura-pir-enhance =0.0.1-rc1 [wallet]`, `zakura-pir-status =0.0.1-rc0`, `zakura-transaction-status =0.0.1-rc0`
  - Voting: `zcash_voting =5.1.1-rc.3` (crates.io; was a git rev), `voting-crypto-deps =0.2.4`, Tendermint light-client verification (`tendermint 0.40.4`, `tendermint-light-client-verifier 0.40.4`, `ics23 0.12`)
  - Tor HTTP: `hyper 1`, `hyper-util`, `hyper-rustls 0.27`, `http 1`; arti comes in through the `tor` feature (`arti-client 0.35.0` in the lockfile)
  - Ledger: `ledger-transport` / `ledger-transport-hid 0.11.0`, desktop only (`cfg(macos/windows/linux)`); mobile Ledger uses native BLE plus a transport-neutral APDU API
  - `tonic 0.14` + `rustls 0.23` (ring provider installed in `init_app`), `tokio 1`
  - `rusqlite 0.37`, `aes-gcm 0.10`, `pbkdf2 0.12`, `zeroize`, `secrecy`
  - `ur` (git KeystoneHQ/ur-rs 0.3.3), `ur-registry` (now git `valargroup/keystone-sdk-rust` rev c2119436; was KeystoneHQ)

**`rust/src` layout: the api/ vs wallet/ split**

| Module | Purpose |
|---|---|
| `lib.rs` | `pub mod api; pub mod ffi; mod frb_generated; pub mod migration_preparation; pub mod network_privacy; mod tor_update_relay; pub mod wallet;` (plus `#![recursion_limit = "256"]`) |
| `api/` | **The FRB surface only.** Flat functions taking primitives, `Vec<u8>`, `String` and simple structs, returning `Result<T, String>`. `simple.rs` (`#[frb(init)] init_app()`), `wallet.rs` (create/import/list/delete accounts, addresses, mnemonic, migrate), `sync.rs` (sync stream, balance, validate address, propose/estimate/execute, PCZT create/prove/redact/extract+broadcast, history), `secret.rs`, `keystone.rs`, `voting.rs`; new: `ledger.rs`, `network_privacy.rs`, `gift_card_tracking.rs`, `voting_session.rs`. |
| `wallet/` | **Internals, not scanned by codegen:** `db.rs`, `keys.rs`, `keystone.rs` (UR session state), `network.rs`, `secret_payload.rs`, `sync/{pczt,send,transactions,broadcast,payment_link,proposal_locks,migration*}.rs`, `sync_engine/{block_source,lwd,mempool,error,tip_cache,address_history,claim_roots,ledger_discovery}.rs` + `sync_engine/enhancement/` (replaces `enhance.rs`), `ledger/`, `voting/`, `addresses/`, `gift_card_tracking.rs`, `wallet_summary_cache.rs`. |
| `ffi.rs` | A hand-written C ABI for the Swift background tasks (now migration preparation only). It sits outside `api/` on purpose so FRB ignores it. |
| `network_privacy.rs` | Process-wide Tor route policy (6.5). |
| `tor_update_relay.rs` | Token-protected loopback HTTP server that streams desktop update assets from arti to Sparkle / Velopack. |

**`rust_builder/`** is the standard FRB cargokit plugin template:
- `rust_builder/pubspec.yaml`: plugin `rust_lib_zcash_wallet`, `ffiPlugin: true` for android, ios, linux, macos, windows. The app depends on it via `rust_lib_zcash_wallet: {path: rust_builder}`.
- `rust_builder/cargokit/` contains `build_pod.sh`, `build_tool/`, `cmake/`, `gradle/plugin.gradle` and `run_build_tool.sh`. Cargokit is MIT-licensed; its `LICENSE` file is kept in that directory.
- **Android** (`rust_builder/android/build.gradle`): `apply from: "../cargokit/gradle/plugin.gradle"` with `cargokit { manifestDir = "../../rust"; libname = "rust_lib_zcash_wallet" }`. Gradle builds the `.so` for each ABI with the NDK; `ndkVersion` is inherited from the app.
  - The app's `android/app/build.gradle.kts` now sets `compileSdk 36`, `ndkVersion "28.2.13676358"` and **`minSdk 30`** (the Ledger Mobile DMK 0.0.4 AAR requires it), Java 17 and `androidx.biometric:biometric:1.1.0`. Reproducible and F-Droid builds: `scripts/build-android-reproducible.sh`, `scripts/build-android-fdroid.sh`, `fdroid/`.
- **iOS** (`rust_builder/ios/rust_lib_zcash_wallet.podspec`): platform iOS 15.0.
  - A script phase `sh "$PODS_TARGET_SRCROOT/../cargokit/build_pod.sh" ../../rust rust_lib_zcash_wallet` runs before compile.
  - `OTHER_LDFLAGS = -force_load ${BUILT_PRODUCTS_DIR}/librust_lib_zcash_wallet.a`.
  - The Podfile uses `platform :ios, '15.0'` and `use_frameworks!`.
- **Build commands:**
  - Development: `fvm flutter run --dart-define=VIZOR_FORM_FACTOR=mobile`.
  - Release: `fvm flutter build ipa|appbundle --dart-define=VIZOR_FORM_FACTOR=mobile --dart-define=VIZOR_RELEASE_VERSION=...` (fastlane lanes in `fastlane/{ios,android}/Fastfile`).
  - Flutter is pinned by `.fvmrc` to **3.47.2** (was 3.41.6 at `ff02152`; PR #638); the Dart SDK constraint is still `^3.11.4`. pubspec sets `flutter: config: enable-swift-package-manager: false` (Cargokit links through CocoaPods).
- **lightwalletd protos:** the Dart gRPC stack (`protos/*.proto`, `lib/src/generated/*.pb*.dart`, `grpc ^5.1.0`) is still in pubspec, but `lib/src` no longer imports it (only integration-test support does). The import birthday estimate moved to Rust (`get_import_birthday_metadata`, `estimate_import_birthday_height` in `api/network_privacy.rs`, with hard-coded mainnet height/time anchors), so every request can go through Tor.

Other notable pubspec dependencies:
- `go_router ^17.1.0`, `flutter_svg ^2.0.10`, `pretty_qr_code ^3.6.0`, `qr_flutter ^4.1.0`, `share_plus ^12.0.1`, `path_provider`, `url_launcher`, `shared_preferences`.
- `mobile_scanner` (git fork `chainapsis/mobile_scanner`, ref `fe622da8`; PR #605 "Use ZXing-C++ mobile scanner fork").
- New: `cryptography ^2.7.0`, `file_selector ^1.1.0` (desktop "Save QR image"), `dependency_overrides: flutter_secure_storage_windows` (local fork). No wakelock or `local_auth` package: keep-awake and biometrics are native channels.
- Dev: `widgetbook ^3.14.3` (galleries now organized, 38 files under `lib/widgetbook/`, run via `widgetbook.sh`), plus `lib/figma_compare.dart` + `scripts/figma-compare.sh` for headless PNG renders compared against Figma.

---

## 8. Copy conventions and tone

From `AGENTS.md` "UI Copy Conventions" (unchanged between `ff02152` and `4bff2e7`), confirmed across the code.

**Casing rules**
- **Sentence case everywhere** for buttons, toasts, sheet titles, labels, errors and empty states. Capitalize only the first word and proper nouns.
- Canonical acronyms and brands keep their casing: `ZEC`, `Zcash`, `Keystone`, `Vizor`.
- Interpolated labels stay sentence case: `'$symbol deposit tx'`.
- **Exceptions:** screen titles, onboarding step titles and Figma display headings may be Title Case. Examples: "Select Recipient", "Review Send", "Secret Passphrase", "Create Passcode", "Welcome Back".
- The code is not perfectly consistent: "Confirm & Send", "Add Memo" and "Continue Anyway" still exist.

**Tone**
- Short, calm and direct, often second person.
- Errors say what happened and what to do: "Couldn't load activity. Try again in a moment."
- Progress labels use a trailing ellipsis made of three dots ("Sending...", "Preparing...", "Reading signature..."). Vizor sometimes adds a space before it ("Loading the QR code ...").
- Privacy is framed positively: "Strong privacy by default.", "Only the recipient can read this".

**CTA vocabulary**

| Situation | Label |
|---|---|
| Next step | "Continue" (often with a trailing chevron) |
| Final step | "Finish & review", "Confirm & Send", "Confirm & import" |
| Hardware | "Next step" (was "Get signature"), "Scan signature", "Show QR" / "Show transaction QR", "Confirm with Keystone", "Confirm with Ledger", "Retry saving" |
| Dismissal | "Cancel", "Close", "Not now", "Back to wallet", "Go back" |
| Retry | "Try again", "Request again", "Retry" |
| Disabled CTA carrying its reason | "Enter address to continue", "Enter amount to continue", "Not enough ZEC" |

**Toasts** are past-tense confirmations: "Address copied", "Passcode updated", "Shielding complete", "Endpoint updated".

**Labels** are nouns: "Amount", "To", "From", "Status", "Message", "Timestamp", "Tx ID", "Tx fee", "Sending to".

**Status words:** "In progress", "Completed", "Failed, funds returned", "Pending". Newer surfaces favour short outcome titles with a plain next step: "Sent!" / "It will confirm on-chain shortly. Track it in Activity.", "Queued to send", "Nothing was sent, your funds haven't moved. Try again.", "Payment status pending" / "Check status".

**Uncertainty is stated, never guessed** (Ledger, Gift Card and Pay flows): "Check Activity before sending again.", "Signature preserved", "Unverified", "Payment status\nuncertain". Hardware stages avoid timers and percentages ("Review and approve when prompted on your Ledger.").

**Audit files:** `qa-copy-review.csv` and `copy-review-20260528-1554.csv` are referenced by AGENTS.md but are **not in this checkout**.

---

## 9. What to copy vs. adapt for Zafe

| Vizor | Zafe | Action |
|---|---|---|
| Token system (`primitives.dart`, `colors/*`, `AppTheme`/`context.colors`, `AppSpacing`, `AppRadii`, `AppTypographyMobile`, sizing) | Same | **Copy** (with attribution). Consider a different brand hue than crimson so Zafe is not mistaken for Vizor; changing the Crimson ladder alone re-skins the brand. |
| `AppButton`, `MobileTextField`, `MobileSurfaceCard`, `MobileListRow`, `MobileReviewInfoRow`, `ReviewWrapCard`, `showAppMobileSheet` + `MobileModalCard` + `MobileModalScaffold`, `AppToast`, `AppIcon`/`AppIcons`, `AppHaptics`, `DotQrShape`, `MobileTopNav`, `AppMobileTabBar`, `AppMobileShell`, `MobileBottomSafeArea`, `MobileTopScrollFade` | Same | **Copy** these nearly verbatim. They are self-contained and depend only on the tokens. |
| Tab shell (Home / Swap / Activity / Settings) | Home (vault) / Activity (incl. pending approvals) / Settings, perhaps an Approvals tab | **Adapt.** Keep `StatefulShellRoute.indexedStack` and the single `_allMobileTabs` list. |
| Keystone account (`isHardware`, UFVK-only watch account, `MobileAccountAvatar` Keystone badge) | FROST vault account (group UFVK, participant role, threshold) | **Direct map.** The vault is a watch-only account whose spends require external signatures. Reuse the badge pattern ("vault" glyph, "t-of-n" chip). |
| PCZT pipeline: propose → `createPcztFromProposal` → redact → *(signing)* → prove in parallel → broadcast. Keystone batch signing now returns **signatures only** (checked for request id and count) that are applied to the wallet-held proven PCZT | Same pipeline; signing = FROST rounds over the blind relay instead of a QR round-trip | **Copy the pipeline and the "prove while signing" trick.** The batch-signing shape (signatures-only response + wallet-held PCZT) is the closest analogue to FROST shares + the leader's proven PCZT. Keep `sendFlowId`, `discardSendProposal` on cancel, validate every signature before any network I/O, and the broadcast status set (`broadcasted` / `broadcast_unknown` / `partial_broadcast` / `broadcasted_storage_failed`). |
| Ledger outbox + input locks (`vizor_ledger_signed_operations`, `proposal_locks.rs`, recovery coordinator, claim registry) | Aggregated signatures and proven tx before broadcast; notes reserved by open proposals | **Adapt.** Checkpoint durably before broadcast; recover `signed_pending_broadcast` / `result_pending_ack` at startup; lock proposal inputs with `zcash_client_backend`'s `OutputLockStore` (already in Zafe's pin) and retain until expiry on an ambiguous broadcast. |
| `MobileKeystonePcztSigningFlow` states `{preparing, showQr, scanning, failed}`; now a full-screen two-step layout. Ledger stages `{preparing, sending, reviewing, finishing}` | `{preparing, awaitingApprovals(k of t), aggregating, failed}` | **Adapt.** Keep the title/subtitle, the one-line instruction slot, a primary action, "Cancel", and pop-null-means-cancel. Replace the QR stage with a signer list (avatar + name + status chip "Approved" / "Waiting"). Take Ledger's rule: stages from real protocol boundaries, no timers or percentages. |
| Send status (`MobileTransactionProgressScreen`: badge, title, body, one action; `sendSuccess` / `sendFailure` haptics) | Proposal sent / queued / failed | Copy the structure with Zafe art; keep the older detail-card vocabulary for the proposal detail screen. |
| ZIP-321 request builder + payment request card with up-front precheck | Grantee payment requests; "propose from request" | **Copy** (12.2). |
| Send wizard (recipient → amount → review → status) | Proposal wizard: create proposal → co-signers approve → broadcast | **Copy the layouts** (floating address field, serif amount, review rows, detail card, status chip). Add an approvals section to review/status. |
| Status chip vocabulary ("In progress" / "Completed" / "Failed, funds returned") | Add "Awaiting approvals (1 of 2)", "Rejected", "Expired" | Adapt. |
| Receive screen (shielded/transparent tabs, dot QR, renew diversified address, "Request ZEC") | Vault receive address | Copy. Consider dropping the transparent tab if vaults are shielded-only. |
| Activity feed sections and rows | Same, plus proposal rows | Copy. |
| Passcode + biometrics (custom channel escrow), privacy overlay (now iOS + Android + macOS capture blocking), sensitive clipboard (iOS + Android), keep-awake | Same needs for local key-share protection | Copy the patterns. The native channel code must be rewritten or ported (Swift/Kotlin handlers). |
| Secure store (PBKDF2 verifier, AES-GCM payloads, rotation journal, iOS `first_unlock_this_device` + migration) | Store FROST key shares instead of mnemonics | Adapt; use `first_unlock_this_device` from the start (shares must not migrate to a new device via backup). Also encrypt the wallet DB, which Vizor still does not. |
| Tor (`network_privacy.rs`: embedded arti, fail-closed, isolated circuit for broadcasts, dormant in background) | Same, plus the relay | **Copy the policy** (tracker "Next up" item 3). |
| Viewing-key export, custom explorer, contact names everywhere | Auditor UFVK export, explorer links, member/payee names | Copy (12.5, 12.6). |
| Mnemonic onboarding (create / import / seed display / birthday) | Not applicable as-is (FROST DKG or share import instead) | **Drop.** Keep the step-scaffold, progress bar and passcode steps; reuse the Keystone "select account" radio cards, the birthday screen and "Customise account" (name + avatar) for vault creation/import. |
| Swap / Pay (NEAR Intents), voting (`zcash_voting`), Gift Cards, Ironwood migration, donation, desktop shell, Windows/Linux updaters | Not needed | **Drop**, but borrow patterns: voting's stage labels and question navigation for proposals, Gift Card groups' batch cost summary for batch payments, the migration outbox for scheduled payments. |
| Zakura crate forks (`zakura-*` renamed packages) | Zafe pins upstream librustzcash (`orchard =0.15.5`, `zcash_client_backend =0.24.0`, …) | Not required. Vizor's code now targets the Zakura APIs; port Rust snippets against Zafe's pins rather than copying. |
| Knight illustrations, profile pictures, Vizor wordmark, crimson shield QR badge | Must be Zafe's own | **Do not copy** (section 10). |

---

## 10. Licensing

**What Vizor's license covers**
- Vizor is **Apache License 2.0** ("Copyright 2026 Vizor contributors", `LICENSE`; README "License: Apache License 2.0"). Unchanged at `4bff2e7`.
- There is still **no `NOTICE` file**, and source files carry **no per-file license headers**. The fonts still ship without OFL texts. `CONTRIBUTING.md` and `SECURITY.md` are new.

**Obligations when Zafe copies Vizor code (Apache-2.0 §4)**
1. Include a copy of the Apache-2.0 license with Zafe's distribution, e.g. `third_party/vizor/LICENSE` or in-app via `LicenseRegistry`.
2. Mark modified files with a prominent notice. Recommended header on each copied or derived Dart file:
   `// Portions derived from Vizor (https://github.com/chainapsis/vizor-wallet), Copyright 2026 Vizor contributors, Apache-2.0. Modified by Zafe.`
3. Keep any existing copyright or attribution notices. There are none in-file today, but keep the cargokit `LICENSE` if its code is copied.
4. Vizor has no NOTICE file, so nothing must be propagated from one. Adding a Zafe `NOTICE` that credits Vizor is good practice.
5. **Trademarks are not licensed (§6).** Do not use "Vizor", the wordmark (`AppIcons.vizor`, `VizorWordmark`), Keplr branding, or confusingly similar trade dress. Also rename channel names (`com.zcash.wallet/*`), bundle IDs (`com.keplr.vizor`), secure-store service names, and every piece of copy containing "Vizor".

**Assets with separate or unclear licensing**
- **Fonts: OFL 1.1, not Apache.** Geist, Geist Mono and Young Serif are verified from the embedded name tables; Inter's embedded strings point to openfontlicense.org. The repo omits the OFL texts, so Zafe must add the OFL license and copyright lines for each bundled family.
  - Obtaining fonts from their upstream releases (vercel/geist-font, noirblancrouge/YoungSerif) is cleaner than copying them from Vizor.
  - The OFL forbids selling the fonts alone and restricts use of the Reserved Font Names in modified versions. Bundling them unmodified in an app is fine.
- **Illustrations and profile pictures** (`assets/illustrations/`, `assets/profile_pictures/`): the bespoke knight artwork has no separate license and is technically covered by the repo's Apache-2.0. It is, however, Vizor's brand identity, and its provenance is undocumented (it may be commissioned or AI-assisted).
  - **Recommendation:** do not reuse it. Commission or create Zafe art for the same slots.
- **Icons** (`assets/icons/`): Apache-2.0 via the repo. They are generic and reusable with attribution, **except** the brand-bearing ones: `vizor.svg`, `keystone.svg` / `keystone_scan*.svg` (Keystone's trademark, which is fine to use nominatively when referring to Keystone), `ledger.svg` / `ledger_brand.svg` (Ledger's trademark, same rule), `tor.svg` (Tor Project mark), `github.svg`, `near_intents_*`, `zcash*.svg` and `network_zec.png` (Zcash marks, governed by the Zcash trademark policy), `ironwood_wordmark.svg`, and the `receive_qr_*` / `payment_link_share_badge.png` badges. `touch_id.svg` / Face ID artwork are Apple glyph-alikes; check Apple's guidelines before reuse.
- **Token and chain logos** (`assets/swap/`): third-party trademarks. Not needed by Zafe.
- **Cargokit** (`rust_builder/cargokit/`): has its own `LICENSE`. Zafe will likely generate its own `rust_builder` with `flutter_rust_bridge_codegen create`/`integrate` rather than copy Vizor's.
- **`third_party/flutter_secure_storage_windows`:** a vendored fork with its own `LICENSE` (desktop only; not needed by Zafe).
- **`mobile_scanner` and `desktop_window_bootstrap`:** git forks under `chainapsis/` (`mobile_scanner` pinned at `fe622da8`, the ZXing-C++ fork). Check each fork's license before depending on it; upstream `mobile_scanner` is BSD-3-Clause. Prefer the pub.dev release unless the fork's fix is needed.

**Practical recommendation**
- Copy the Dart design-system and widget code with headers plus a Zafe `NOTICE`.
- Take fonts from upstream with their OFL files.
- Do not copy art or brand assets.
- Re-derive the Rust side against Zafe's own crates rather than copying `rust/src`, which is heavily Vizor-specific.

---

## 11. Resync notes (Zafe's copied files vs. upstream)

Zafe copied `lib/src/core` from `ff02152` (see `app/NOTICE`). Method: for every file in `app/lib/src/core` that also exists upstream, compare `ff02152` with `4bff2e7`, and `ff02152` with Zafe's copy. Zafe's own edits to copied files are small: channel names renamed to `xyz.zafe/*` (`app_theme_host.dart`, `app_haptics.dart`), `AppIcons.vizor` removed (`app_icon.dart`), "Zafe adds no extra fee." (`mobile_tx_fee_info_sheet.dart`), the `AppButton` semantics fix, and a rewritten `config/network_config.dart`. Zafe-only files (`errors/zafe_error_copy.dart`, `formatting/member_label.dart`, `layout/mobile/zafe_screen.dart`, `privacy/amount_display.dart`, `storage/zafe_paths.dart`, `storage/zafe_secure_store.dart`, `widgets/mobile/zafe_detail.dart`) have no upstream counterpart.

**Unchanged upstream (nothing to resync):** `layout/app_form_factor.dart`; `layout/mobile/{app_mobile_shell, app_mobile_tab_bar, mobile_bottom_safe_area, mobile_top_scroll_fade}.dart`; `motion/onboarding_motion.dart`; `privacy/privacy_mask.dart`; `theme/{app_icon_size, app_radii, app_serif_display, app_shadows, app_sizing, app_spacing, app_theme, app_theme_data, app_theme_host, app_typography, primitives}.dart`; `theme/colors/{app_colors, app_fade_colors, app_icon_colors, app_macos_utility_colors, app_nav_panel_colors, app_shadow_colors, app_state_colors, app_sync_colors, app_text_colors}.dart`; `widgets/{app_copy_feedback, app_tappable}.dart`; `widgets/mobile/{mobile_review_row, mobile_surface_card, unsupported_sheet}.dart`.

**Changed upstream** (A = additive, safe to take; B = behavioral or visual; R = rename or removal that breaks callers):

| Zafe file (`app/lib/src/core/...`) | Kind | What changed upstream |
|---|---|---|
| `theme/colors/app_background_colors.dart` | A | `utilityDestructiveStrong` (plum p300Dark / p400Light). Needed by the destructive toast. |
| `theme/colors/app_border_colors.dart` | A | `utilityPositiveStrong` (green p300Dark / p500Light). |
| `theme/colors/app_button_colors.dart` | B | Primary button dark `border`: `p150Alpha15Dark` → `p900Alpha10Dark` (white @ 10%). |
| `theme/colors/app_surface_colors.dart` | R | `input` / `inputFocus` → `input` is `AppInputSurfaceColors{primary, secondary, focus}`. Zafe callers of `colors.surface.input` (today only `mobile_text_field.dart`) must change to `.input.primary`. |
| `theme/legacy_material_theme.dart` | B | iOS-only `dialogTheme` with `RoundedSuperellipseBorder(AppRadii.xLarge)`. |
| `formatting/zec_amount.dart` | A + R | Adds `compactBalancePretty({min 2, max 5, hideZeroFraction: true})` / `.compactBalance` (home totals capped at 4–5 decimals; tiny values show `<0.00001`) and a `hideZeroFraction` option on `pretty()`. **Removes `ZecAmountInputFormatter`**, replaced by `widgets/decimal_amount_input_formatter.dart` `DecimalAmountInputFormatter({required maxFractionDigits, maxLength})` (also turns a leading `.` into `0.`, PR #678). Zafe's `features/send/send_screen.dart` uses `ZecAmountInputFormatter`. |
| `formatting/address_display.dart` | A | Doc comment only (`addressVerifyGrid` kept as legacy). |
| `feedback/app_haptics.dart` | A | `votingAnswersComplete()`, `sendSuccess()` (native `sendSuccess` on the `haptics` channel: 30 ms pulse, then 40 ms full pulse after 60 ms; non-iOS fallback medium → light → selection), `sendFailure()` (native four pulses over 290 ms; fallback light). Needs the iOS native handler to get the custom patterns. |
| `layout/mobile/app_mobile_sheet.dart` | B | Bottom gap **32 → 16** (`AppSpacing.sm`) on both platforms (PR #729). On iOS, non-transparent sheets push a `PreparedModalSheetRoute` (laid out offstage until corners are resolved) and `MobileModalCard` uses iOS continuous corners (`appModalShape` → `RoundedSuperellipseBorder`) that follow the display's corner radius via `MobileModalCorners` + the native `com.zcash.wallet/modal_corners` channel (`ios/Runner/ModalCornerHandler.swift`). New `enableDrag` param; `MobileModalCard({margin, followsScreenCorners = true})` and static `bottomGapFor(context)`. Pulls in three new files: `layout/mobile/{mobile_modal_corners, prepared_modal_sheet_route}.dart`, `widgets/app_modal_shape.dart`, plus `services/native_modal_corners.dart`. |
| `layout/mobile/mobile_top_nav.dart` | A | `titleMaxLines = 1`, `foregroundColor` (tints title and back icon). |
| `widgets/app_button.dart` | A | `growWithContent` (height becomes a minimum), `borderRadius` (non-pill rows; focus ring follows), `enabledBackgroundColor`, `pressedBackgroundColor`, `enabledLabelColor`, `pressedLabelColor`. **Upstream still has no `Semantics(button: true, onTap:)` wrapper** (its `AppButton` is `MouseRegion` + `GestureDetector` over the focus shell; only `AppTappable` adds semantics). Keep Zafe's `MergeSemantics(Semantics(button, enabled, onTap))` fix when merging. |
| `widgets/app_icon.dart` | A | 26 new `AppIcons` constants (section 2.2); copy only the SVGs Zafe uses. |
| `widgets/app_loading_icon.dart` | B | Honors reduced motion: stops the spoke animation when `MediaQuery.disableAnimations` is set (checked in `didChangeDependencies`/`build`). |
| `widgets/app_toast.dart` | A + B | `AppToastTone {neutral, destructive}` on `AppToast`, `showAppToast` and the fallback host; destructive = `background.utilityDestructiveStrong` fill, white text/icon, label weight 400. Icon size 16 → 20 for all toasts. |
| `widgets/dot_qr_shape.dart` | none | Formatting only. |
| `widgets/mobile/mobile_address_verify_sheet.dart` | B | The 5-character chunk grid with crimson highlights and "Cancel" is gone. Now `MobileAddressVerifySheet`: title, the full address as continuous wrapping Geist Mono (`FullAddressText` from the new `widgets/full_address_viewer.dart`), a primary "Copy address" button (`FullAddressCopyButton`) and a header close (PR #644). Zafe uses the sheet in `proposal_screen.dart` and `send_screen.dart`. |
| `widgets/mobile/mobile_list_row.dart` | A | `labelBadge` widget after the label (settings "New" pill, `settings/widgets/settings_new_badge.dart`). |
| `widgets/mobile/mobile_tx_fee_info_sheet.dart` | A | `title` / `description` params (defaults are the old copy, which says "Vizor adds no extra fee."; keep Zafe's text). |
| `widgets/mobile_text_field.dart` | B | `enabled` param (unfocuses when disabled); tapping anywhere on the shell focuses the field and moves the cursor to the end (PR #316); fill reads `surface.input.primary`. |
| `config/network_config.dart` | n/a | Upstream added Ironwood regtest activation height, fast-testnet-migration and "Ironwood masquerade" dart-defines. Zafe's file is rewritten; nothing to take. |

Suggested order for a deliberate resync: tokens first (background, border, button, surface with the `input.primary` rename), then `mobile_text_field`, `app_toast`, `app_loading_icon`, `mobile_top_nav`, `mobile_list_row`, `app_button` (additive params only, keep the semantics wrapper), `zec_amount` + `DecimalAmountInputFormatter`, then decide on the full-address sheet (Zafe verification UX) and the iOS modal-corner machinery (needs a Swift handler; only worth it once iOS builds exist).

---

## 12. New upstream features since `ff02152`

Short descriptions with entry points, for deciding what to borrow. Hardware signing (Ledger, Keystone batch signing) is in section 5, Tor in 6.5, keep-awake and capture protection in 6.4.

### 12.1 Gift Cards (payment links; `lib/src/features/payment_links/`, 64 files)

- **Model:** a card is a throwaway 24-word wallet. The sender funds it and shares a link carrying the seed; the recipient opens it, sees whether it can be claimed, and sweeps it into their own account ("Redeem the Card"). Screens: `payment_links/screens/payment_links_screen.dart`, `payment_links_mobile_body.dart`; settings row "My gift cards".
- **Copy:** "Create Gift Card", "How Gift Cards work", "Card fee (deposit + redeem)", "Total amount deducted", "Gift Card is\nalmost ready!", "Ready to share", "Paste card link", "The link doesn’t look legit." Artwork from `assets/illustrations/payment_links/` (knight universe; do not copy).
- **Compact links** (`docs/compact-gift-links.md`, `models/compact_payment_link_codec.dart`): `https://link.vizor.cash/payment-links/open#v3=<base64url JSON array>`, positional `[network, entropy, birthday, zatoshi, artwork, usdSnapshot, message, label]`. The payload lives in the **URL fragment**, so the link server never sees the secret; 16 KiB cap; v1/v2 still read; message ≤ 128 characters / 512 bytes.
- **Groups** (`docs/gift-card-groups.md`, `widgets/payment_link_bulk_desktop_flow.dart`, `payment_link_batch_*`, desktop only): 2–50 cards (2–30 on hardware) in **one funding transaction and one hardware signature**. Cost rows "Per card", "× N cards", "Network fee", "Total"; the fee is estimated for the whole output set, never one-card fee × N. Button "Create N cards". An unknown broadcast result shows "Payment status pending" / "Check status" and never offers a second Create; a failed save offers "Try saving again". Detail view groups cards as Pending / Unused / Used, with "Save all links as CSV" (`services/payment_link_batch_export.dart`).
- **Claim outcomes** (`docs/gift-card-claim-outcomes.md`, `rust/src/wallet/sync/payment_link.rs`): "Already claimed" only with positive evidence that the funds were spent elsewhere (6 scanned confirmations, bounded by pending scan ranges); an empty history never counts. States "Claim failed", "Checking result"; "Check status" scans without rebroadcasting. Crash-safe via `services/recovery_store` / `recovery_reconciler` (encrypted records).
- **Usage tracking** (`docs/gift-card-usage-tracking.md`, `rust/src/wallet/gift_card_tracking.rs`, `services/gift_card_tracking_service.dart`): each created card's UFVK is imported as `AccountPurpose::ViewOnly` into a **separate observer DB** (no seed stored). States unknown → unused → spendDetected → used (6 confirmations); UI "Checking…", "Confirming", "Unverified" (tap for the reason). Refreshes coalesce with a 30 s cooldown; no global timer.

### 12.2 ZIP-321 payment requests, incoming links and Pay

- **Create a request** (Receive → "Request ZEC" on mobile; `receive/widgets/mobile/receive_request_sheet.dart`, `receive/widgets/request/*`, `receive/services/zec_request_draft.dart`, `request_qr_export.dart`): amount step (USD/ZEC toggle, optional "Message" with hint "Anyone you send the link to can read this", shielded only, "Create request"), then a result step ("Share request", "Copy link" → toast "Request link copied", "Save QR image"). Errors "Enter up to 8 decimals", "Amount exceeds the ZEC supply", "Enter an amount like 0.5". The receive address is snapshotted when the flow opens.
- **Builder** (`lib/src/core/zcash/zip321_payment_request_builder.dart`): one recipient, amount, optional memo; **`label` and `message` are never emitted** (they would travel in clear to whoever sees the link); typed `Zip321BuildException` kinds (address, amountFormat, amountDecimals, amountSupply, memoTooLong; "Transparent payments cannot carry a message.").
- **Incoming links** (`lib/src/services/incoming_uri_service.dart`, channel `com.zcash.wallet/payment_uri`; routing in 4.2): the host is classified before the scheme so a gift link's mnemonic never reaches the ZIP-321 parser or a log; `vizor_deep_link.dart` is an exact route allowlist on a trusted HTTPS origin; `payment_uri_drain_policy.dart` is a pure, unit-tested decision table for locked / onboarding / stale / no-wallet states. `dev/open_payment_uri.sh` and `dev/payment_uri_demo.html` exercise it.
- **Payment request card** (`send/widgets/payment_request_{host,card,surface}.dart`, `providers/payment_request_flow_provider.dart`): an overlay above the router (not a route) that owns the request until answered ("Review" / "Edit" / "Cancel"; swipe to dismiss on mobile). `send/services/payment_request_precheck.dart` validates and **builds the real proposal up front**, so Review opens with the fee computed. Copy: "Payment request", "Requester" (tooltip "Name supplied by the payment link. Vizor can't verify who sent it."), "Note from requester", "Transaction memo", "Replaced an earlier link", "Recipient address doesn't look right", "Not enough ZEC for this amount and the network fee", "Wallet is still syncing — this will update when it finishes", "Couldn't check this request". QR scans and pasted input feed the same flow (PR #661, #667).
- **Pay** (`lib/src/features/pay/`): pays ZEC to other chains through NEAR Intents 1Click quotes. Wizard "1 Amount > 2 Recipient > 3 Review" (`pay_wizard_stepper.dart`, completed steps tappable), "Choose payment asset", "Quote expires in", "Refresh quote", "Confirm & pay", "Recently sent" (`models/pay_recent_recipients.dart`, never guesses between duplicate contacts), "New address detected. Add to contacts" (`pay_add_contact_modal.dart`). Results "Payment\nSubmitted" / "Payment status\nuncertain" (`pay/screens/mobile/mobile_pay_submitted_screen.dart`). Cross-chain payment requests (PRs #655–#660) resolve into the Pay lifecycle.

### 12.3 Coinholder voting (`lib/src/features/voting/`, `zcash_voting` SDK)

Mobile screens (PRs #544, #562, #571): polls, proposal detail, review, submission confirmation, status, results, `mobile/mobile_voting_submission_progress_screen.dart`. UX patterns:
- Named submission stages: "Preparing voting power", "Proving voting authority", "Casting votes", "Delivering your responses", "Finalizing submission".
- `widgets/voting_auto_advance_indicator.dart`: a countdown ("Moving to the next unanswered question") instead of a spinner.
- Navigation (`voting_proposal_navigation.dart`, `voting_proposal_list.dart`): "Tap a question to jump to it.", "Unanswered questions", "Skip unanswered questions?", "Review your answers", "Confirm & submit".
- Resume (`voting_resume_plan.dart`): "Resume to complete the submission.", "Submission not complete". Results "Tallying", "Results pending...", "View results". Eligibility "Not eligible for this round" / "Switch to an eligible account to vote."
- Home discovery (`docs/voting-home-discovery.md`): the card appears only after positive confirmation, decisions are persisted, and discovery runs on entry/foreground, never on a polling timer. Participation check (`docs/voting-participation.md`) is light-client verified with backoff 1, 2, 4, 8, 16 … capped at 30 minutes.

### 12.4 Ironwood migration (`lib/src/features/migration/`, 56 files)

- Moves Orchard funds into the Ironwood pool. Options: "Immediate" ("Fast (~10 mins) but less private.") or private ("Splits transactions into multiple parts…", randomized denominations spread over time). The schedule screen shows "Rounds remaining", "Next round", "Est. completion", "Current block", "Ready in", "Ready to sign". Keystone signs whole batches in one QR session; the private option is unavailable for Ledger accounts (`privateUnavailableForLedger`).
- Guidance: `core/widgets/app_carousel.dart` status carousel, `migration/widgets/ironwood_migration_announcement_modal.dart`, home banner and attention sheets (4.4).
- **Background broadcast of pre-signed transactions** (iOS): `ios/Runner/BackgroundMigrationOutbox*.swift` is a durable outbox of fully signed raw transactions, each with `scheduledHeight` and `expiryHeight`; states staged → armed → submitting → accepted / rejected / expired / needsResign (each with an "AwaitingReconciliation" variant). `BackgroundMigrationManager.swift` uses `BGProcessingTaskRequest` (`com.keplr.vizor.ironwood-migration`) to broadcast items once the tip reaches `scheduledHeight`; `BackgroundMigrationPreparationManager.swift` uses continued processing plus a Live Activity. Android: WorkManager over JNI.
- **Local notifications without a push server** (`ios/Runner/MigrationPreparationNotificationCoordinator.swift`): wave confirmed / needs foreground / terminal failure, deduplicated per scope and kind, one aggregated alert. Copy: "Confirming in the background. You can close Vizor."
- Input locking for in-flight migrations uses the same `OutputLockStore` API as hardware sends (`rust/src/wallet/sync/migration_wallet_ops.rs`, `proposal_locks.rs`).

### 12.5 Address book

- Send autocomplete (4.6), contact names on every address surface (`address_book/models/address_book_label_lookup.dart`, `widgets/contact_name_inline.dart`; exact match, never choosing between duplicates), automatic labels (`contact_label_generator.dart`), "New address detected. Add to contacts" after Pay.

### 12.6 Settings and smaller features

- **Viewing key export** (PR #489; `settings/screens/mobile/mobile_viewing_key_screen.dart`, `settings_viewing_key_screen.dart`, `settings/viewing_key_copy.dart`): passcode gate, then the UFVK. Copy: "A viewing key gives read-only access to this account. A compatible wallet or service can use it to see your balance and transaction activity, including future transactions, but cannot spend your funds." / "Keep this key private and only share it with people or services you trust."
- **Custom block explorer** (PR #598; `providers/zcash_explorer_provider.dart`, key `zcash_explorer_url`; `core/config/zcash_explorer.dart`): default CipherScan (`cipherscan.app`, testnet/regtest `testnet.cipherscan.app`); a custom template accepts `{txid}`, `{txHash}`, `{tx_hash}` or `{hash}`, and a bare origin becomes `/tx/{txid}`.
- **Private queries / PIR** (PR #601; `providers/enhance_pir_provider.dart`, `settings/widgets/enhance_pir_privacy_control.dart`, Rust `set_enhance_pir_enabled`, `wallet/sync_engine/enhancement/`): fetches transaction details and status by private information retrieval instead of revealing txids to lightwalletd (default endpoint `https://enhance-pir.valargroup.dev`); each lookup goes to PIR or public lwd, never both. Install-scoped preference. Copy (mobile): "Experimental. Queries and enhances transaction data without revealing their IDs to servers."
  **Update 2026-10-14 (upstream `4a859be`, 2026-10-13; not studied in full, Zafe has not adopted any of it):** the one "Private queries" toggle now also covers **transparent PIR** (PRs #971, #944, #947, #977; `docs/private-queries.md`): private recovery of transparent history, UTXOs and tx details, with no public fallback when the service is down, and a durable policy (turn-off needs confirmation and survives restarts). #977 adopts schema v12 (history pages sized apart from the address directory, `/v2` routes, one-query tx details, 8-bit profiles); #942/#955 shrink queries (dithered 44-bit, prefix-only uploads, about 38% less Enhance upload). Everything sits on the **Zakura wallet-libraries forks** (`zakura-client-backend`, `zakura-client-sqlite`, `zakura-pir-enhance`, wallet-pir servers), not on crates.io librustzcash, and #971 calls itself an integration for review with release qualification still open (migrations, mobile/Tor bandwidth). Docs: `zakura-core/wallet-libraries` `docs/transparent-pir-wallet-integration.md`, `valargroup/wallet-pir`. Why Zafe doesn't use it: see `docs/tracker.md` (PIR idea).
- **Wallet link** (`features/wallet_link/`): desktop encrypts an account package (AES-256-GCM, one-time key in the QR) and uploads it to a backend with status / complete / revoke endpoints; the code expires after 1 minute; the phone scans it. Copy: "Link Vizor Mobile", "Only your phone can decode it.", "Time’s up", "Generate new code".
- **Transaction resubmission** (`docs/transaction-resubmission.md`): automatic rebroadcast of unmined transactions; a transaction mined earlier is never rebroadcast during a rewind (`vizor_mined_transactions` trigger; tip height and hash must be unchanged). Expired transactions are not rebuilt.
- **Transparent UTXO recovery** (`docs/transparent-utxo-recovery.md`, PR #699): recovers pre-birthday transparent UTXOs.
- **Donation** (`features/donation/`, desktop only, mainnet only, "Support Vizor").
- **Proving-key warm-up** before send (`send/services/send_proving_key_warmup.dart`).

### 12.7 Tooling

- Widgetbook galleries reorganized (PRs #674–#676; `lib/widgetbook/`, 38 files incl. `fixtures/`), with UI operations made injectable (PR #673).
- `lib/figma_compare.dart` + `scripts/figma-compare.sh widget --scenario X --theme dark|light [--form-factor mobile]` render a scenario to PNG for Figma comparison (CONTRIBUTING.md).
- App-level regtest E2E: `integration_test/*` (payment URI, payment links, migration, voting, Ledger Speculos) driven by `scripts/e2e/flutter-ios-regtest-mobile-*.sh` and `docker-compose.zcash-ironwood-regtest.yml`.
- Reproducible Android and F-Droid builds (`scripts/build-android-reproducible.sh`, `scripts/build-android-fdroid.sh`, `fdroid/`). The repo has no `.github/workflows`.

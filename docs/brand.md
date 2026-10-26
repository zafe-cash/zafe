# Brand: colour, icons and logo

Research and decisions behind Zafe's identity (2026-10-01). The palette moves from lime
("Signal") to **Verdigris**; the app icon moves from the lime tile Z to **Seam**. The
process that produced this is packaged as two project skills:
`.claude/skills/brand-palette` and `.claude/skills/logo-design`.

Working pages (private claude.ai artifacts, owner can share):
- Accent study: https://claude.ai/artifact/LnR2kW6VeREjY5W3Rh5Kis
- Logo study: https://claude.ai/artifact/5K8wWhR8B8WGowhVqFUFxB

## 1. Brief

| | |
|---|---|
| Who | DAO and grant committees and small teams co-signing payments from a shared shielded vault. Many already hold other wallets and use Zcash. |
| Job | Approve or send money with others, slowly and on purpose. A payment waiting for your vote must stand out, and so must money arriving. Nothing should feel like a trading app. |
| Attributes | Collective (nothing moves without the quorum), private, deliberate and calm, part of Zcash yet its own product. |
| Rules out | Neon "go" colours (trading), bank blue, Zcash amber as the brand (Zodl owns it). |

## 2. Category audit (colour)

Hues are OKLCH, measured from published hex values. "Confirmed" means checked against the
product's source code or official release; the rest were scraped from sites or are
secondary sources.

| Brand | Hex | Hue | Source |
|---|---|---|---|
| Safe{Wallet} | #12FF80 (dark #0CB259) | 151 | Confirmed: safe-wallet-monorepo `dark.ts` |
| MetaMask lime | #B9F302 | 126 | Confirmed: metamask-design-system v69 notes |
| Ramp | #E5FE54 | 117 | Unverified |
| Squads | #00D436 | 145 | Site |
| Zcash | #F4B728 | 83 | Confirmed (zcash/logos) |
| Zodl (was Zashi) | #FCBB1A, obsidian #231F20 | 82 | Confirmed: `ZashiColorPalette.kt` |
| Keystone / Ledger / Brex / Monzo | #F5860B / #FF5300 / #FF3D00 / #FF4F40 | 29-58 | Sites |
| Coinbase | #0052FF | 263 | Confirmed |
| Signal / Mercury / Proton / Phantom | #3B45FD / #5266EB / #6D4AFF / #AB9FF2 | 271-290 | Sites; Phantom confirmed |
| Tor | #7D4698 | 313 | Unverified |

Crowded: blue-violet 255-315 (trust and privacy), orange-red 30-60 (hardware wallets),
green-lime 112-156 (multisig and neon), amber ~83 (Zcash, Zodl). **Open: teal to cyan
(~180-220) and magenta to rose (~330-360).** Zafe's old lime was ΔE 0.04 from MetaMask's
2026 lime, and dark UI + neon green + multisig is Safe's pattern.

## 3. How the palette is built

Sources: Stripe "Designing accessible color systems" (lightness curves in a perceptual
space, contrast guaranteed by step distance); Linear's LCH redesign (a theme is base +
accent + contrast); Material 3 HCT (tones per role, `ColorScheme.fromSeed`); Radix Colors
(12 steps, each with a job); Atlassian (brand, accent and semantic colours kept apart;
never use an accent where colour carries meaning); IBM Carbon (one interactive colour).

Rules we applied:
1. **Three colour jobs.** Brand = actions and "needs you". Zcash gold = value (the ticker,
   money arriving, keys; same meaning as the onboarding art in `docs/illustrations.md`).
   Status = warning and error. One colour never means two things.
2. **Contrast from lightness.** Every role sits at a fixed OKLCH lightness, so any hue
   passes WCAG AA (4.5:1) by construction. Neutrals are the app's existing ladder
   lightnesses with ~1% of the brand chroma.
3. **Each hue at its natural value.** Light hues (lime, gold) keep dark labels in both
   themes; teal becomes a deep fill with white labels in the light theme.
4. **Status hues keep ≥35° from the brand** and from gold, picked by complement or
   split-complement.

### Verdigris tokens (hue 188)

Received money uses the value colour (gold), not green: teal and green are only 38° apart.

| Role | Dark | Light |
|---|---|---|
| window | #080B0B | #F1F5F5 |
| ground | #111515 | #FFFFFF |
| raised | #1E2323 | #E5EAE9 |
| line | #272E2D | #D9DFDF |
| text muted / secondary / primary | #737F7D / #9DA9A7 / #F1F5F5 | #7E8987 / #55615F / #090E0E |
| brand fill (primary button) | #51DDD2, label #001B19 | #00736C, label #FFFFFF |
| brand accent (icons, "needs you", focus) | #51DDD2 | #00736C |
| value = received (Zcash gold) | #F3BA3C | #835A00 |
| sync text (glow): green in sync, orange on failure | #6BD99A (#2E8A57) · #FFAE81 | #1E7A47 (#4CB57D) · #A55115 |
| warning (orange, hue 50) | #FFAE81 | #A55115 |
| error (rose, hue 350) | #E96CAD | #A82571 |
| vault card | ink #091312 | light card #E0F3F1 (see below) |

Brand ramp (dark, low → high): `#00110F #001B19 #002825 #003633 #004743 #006660 #51DDD2
#61EADF #70F8ED #8EFFF5 #C7FFF9 #EBFFFD`. Light: `#EBFFFD #BEFFF8 #9EF4EC #84E1D9 #47B9B0
#00736C #00605A #004D49 #003C38 #002B28 #001B19 #000C0B`. Lowest contrast of any text
pair above: 4.98:1. The derivation lives in `.claude/skills/brand-palette/oklch.js`.

### Vault card

**Decision (user, 2026-10-27): plain.** The balance card and the payment card use the
surface every other card uses (`background.ground`: #111515 dark, #FFFFFF light), no
dial, no border. The label is a grey Zcash mark and "Vault balance" in the secondary
text colour (no network badge, shield or capitals); the ticker is the value colour.
Chosen from renders of the dial, a glow, a tinted card and plain (the earlier light
card #E0F3F1 with a dial was decided 2026-10-01). Tokens: `AppVaultCardColors`.

## Type

**Decision (user, 2026-10-27): six sizes, three families with one job each**, after reading
research (legibility and reading studies) and a side-by-side comparison of four directions:
- **Space Grotesk** for amounts and headings: hero 44/48 (balance, a payment), title 28/34,
  heading 22/28 (screen titles), amount 18/24 (rows, cards). Every amount uses tabular
  figures (`kTabularFigures`), so digits don't shift as values change. Ticker: half the
  hero (22), 14 next to an 18 amount, in the value colour.
- **DM Sans** for text: 18/24 subheadings, 16/24 body, 16/20 labels, 14/20 small text.
  **14 is the minimum**: at 13 the x-height falls below the critical print size where
  reading slows (Legge & Bigelow 2011). DM Sans has no tabular figures, so amounts never
  use it.
- **JetBrains Mono** 14/20 for addresses and codes (dotted zero, distinct Il1).
- Amounts shrink to fit (`FittedBox`), never "…": the exact value must stay readable at
  2x text. Steps under ~10% (13 vs 14) don't read as levels; use weight or colour.
- `scripts/check_typography.py` (CI) keeps code on the scale.
- Worth testing with people before mainnet: grouped vs plain addresses (spotting a
  look-alike that matches only the ends), 14 vs 15 small text with presbyopic readers.

## 4. Icon style: Patina two-tone

Phosphor's rule: one default weight, filled only for state. Stroke weight follows the
body font; corners follow the UI radii.
- Idle: 1.7 px rounded outline in the text colour; closed shapes filled with the brand
  accent at ~38% (the "patina").
- Active (current tab, a payment that needs your vote): solid brand accent.
- Icon tiles are circles. Received tiles use the value colour.
- Today's icons are filled Material Symbols everywhere, so the active state is carried by
  colour only.

## 5. Logo research

Process (identity practice, e.g. Chermayeff & Geismar & Haviv, *Identify*): brief →
3-5 attributes → metaphor map → many sketches → reduce → test small and in one colour →
present one. Criteria: **appropriate, distinctive, simple** (simplicity first). Paul Rand:
a logo identifies, it doesn't explain. Bierut: a new mark is an empty vessel.

Platform constraints: no text in the icon (Apple HIG); iOS 26 derives Clear and Tinted
from Default, Dark and Mono layers; Android adaptive icons are 108 dp with a 66 dp safe
circle, an optional monochrome layer (auto-themed on Android 16 QPR2+ if missing); the
notification icon is a 24 dp alpha-only silhouette; ≥2 px strokes at 24 px.

Tropes avoided: shields, padlocks, keyholes, keys, a Z in a circle (reads as the ZEC coin),
hexagons, purple-blue gradients, mascots, Zcash gold in the mark (reads as a token).
Neighbours: Safe's soft geometric S (black + neon green); Zcash's Z with a currency bar in
a gold disc; Chase's octagon of interlocking wedges shows parts-that-lock reads as vault
and cooperation.

Concepts drawn: **Seam** (chosen), **Quorum Z** (three separate strokes, one per member;
good story, gaps ~1 px at 24 px), current Z (only a letter), Dial (dropped: read as a
stopwatch and a Z-in-circle).

### Seam geometry

100-unit tile. Block 22..78 square. A Z-shaped channel, a polyline
(19,37) → (63,37) → (37,63) → (81,63) with mitred joins, splits it into an upper and a
lower piece. Patina: channel 9.5 units wide, outer corners radius ~6.4, channel corners
~1.3. The upper piece is patina pale #D9F5F2, the lower piece brand #51DDD2, on a deep
verdigris tile #004A46. Monochrome, notification and tinted versions are both pieces in one
colour. Wordmark: "zafe", Space Grotesk SemiBold, lower case.

## 6. Decisions and deferred ideas

- Adopted: Verdigris palette, Patina icons, Seam icon, light vault card in light mode
  (user's choice), gold for received money.
- **Kept, not replaced:** the per-vault picture emblems (`features/vaults/vault_emblem.dart`,
  landed 2026-10-01 in another session). The logo study proposed an N-of-M ring emblem for
  the avatar. The threshold already shows as signer dots on the balance card, so the
  ring stays an idea *(tracker)*. The emblem palettes may need retinting to sit with
  Verdigris.
- Quorum Z stays a secondary mark (e.g. a splash animation: three strokes arriving).

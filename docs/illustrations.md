# Illustrations

Zafe's onboarding art is **original work**, generated from hand-written Python that emits
SVG (`scripts/illustrations/`). It takes its *mood* from Vizor's onboarding (a dark,
detailed, ink-drawn scene behind the UI that fades into the window colour) but none of its
content. Never copy, trace, recolour or derive from Vizor's image files, and don't use
knights, castles, Vizor characters, the Vizor name or its wordmark (see `app/NOTICE`).

## Style guide

What we took from Vizor (style only): comic/engraving ink line work, hatch and halftone
texture instead of soft shading, a muted stone/metal neutral palette with one warm accent,
lots of detail in the subject and a quiet area where text sits, art that bleeds off the
screen edge and dissolves into the background.

**Palette** (`PALETTES` in `scripts/illustrations/zafe_art.py`; values come from
`app/lib/src/core/theme`):

| token | dark | light | use |
|---|---|---|---|
| `sky0` / `sky1` | `#111515` / `#080B0B` | `#FFFFFF` / `#F1F5F5` | background gradient; `sky1` = `background.window` (must match exactly) |
| `stone` / `stone2` / `stone3` | `#1E2323` / `#272E2D` / `#161B1B` | `#D9DFDF` / `#E5EAE9` / `#C9D0CF` | masonry, ground, faces (raised and line tones) |
| `metal` / `metal2` | `#353E3D` / `#4E5857` | `#C9D0CF` / `#E5EAE9` | vault doors, fittings |
| `hi` | `#737F7D` | `#FFFFFF` | highlight strokes on top-left edges |
| `ink` | `#020404` | `#27302F` | outlines |
| `dot` | `#000000` | `#55615F` | texture (hatch, stipple), vignette |
| `paper` / `light` | `#9DA9A7` / `#4E5857` | `#FFFFFF` / `#F1F5F5` | envelopes, pages, word cards |
| `brand` / `brand2` / `brand3` | `#51DDD2` / `#006660` / `#003633` | `#00736C` / `#00605A` / `#9EF4EC` | members and brand marks (verdigris); `brand3` for large brand areas (the join door leaf). Light mode uses the deep verdigris family for contrast |
| `gold` / `gold2` / `gold3` | `#F4B728` / `#B8841A` / `#FFE39A` | `#F4B728` / `#C98A10` / `#FFF1C4` | Zcash gold: keys, coins, light (`goldg` gradient) |

Colour meaning: **gold is what matters** (keys, funds, light), **verdigris marks members** and
the brand (one gem/grip per member), neutrals (the app's grays, tinted ~1% toward
verdigris) carry everything else. Keep gold to a few focal areas. No crimson, jade or
lime anywhere (earlier accents).

**Motif**: the vault dial from the home screen, concentric rings with tick marks
(`dial()` in `zafe_art.py`). Use it as a quiet backdrop or on doors and locks.

**Toolkit helpers** (`zafe_art.py`): `stone_wall`, `coin`, `dial`, `key` (a small gold key
share with a brand gem, or a dashed `ghost`), `sparks` (gold glints in one path). Scene-level
props in `scenes.py`: `envelope`, `dial_lock` (combination padlock = passphrase),
`word_tiles`, `ledge`.

**Shapes and line**: bold closed silhouettes with 3-5 px ink outlines (`stroke-linejoin:
round`) at 1080 px width; 2-3 px for secondary detail; highlights as short `hi` strokes on
upper-left edges. Slight irregularity (jittered stone corners, cracks) from a seeded
`random.Random`, so output is deterministic.

**Texture**: `fill="url(#hatch)"` (45°, shadows), `hatch2` (opposite diagonal, lighter
shade), `dots` / `fine` (halftone stipple), `grit` (stone specks and scratches), drawn over a
shape with an `opacity`. These look like SVG patterns but `svg()` bakes them into clipped
geometry: **flutter_svg renders `<pattern>` fills as vertical stripes over the whole shape**.
Paths that use a texture need a `bb="x0 y0 x1 y1"` attribute.

**Light**: radial gradients only (`#glow` gold, `#glowl` brand), no filters or blur.
Finish every scene with the `#vig` vignette.

**flutter_svg limits**: no `<pattern>`, `<polyline>`, `<line>`, `<text>`, filters, blur or
CSS. Use `<path>` (`line_d()` for open polylines), `<circle>`, `<rect>`, `<polygon>`,
gradients and `clipPath`. Always check with `preview.sh`, which renders through flutter_svg;
a browser shows things the app won't.

**Composition**:
- Hero (full-screen, top of screen): 1080 x 1240. Subject centred around y 450-560 and kept
  above y ~620; the bottom 60 % fades out, and the top 10 % sits under a status-bar scrim.
- Banner (top of a form): 1080 x 560, shown with rounded corners (`AppRadii.large`), full
  content width. Keep the subject inside the middle 80 %.
- Small banner (empty states inside a card): 1080 x 440.
- Full-page background: 1080 x 2340, drawn with `BoxFit.cover` behind centred content.
  Put detail at the top and bottom edges; keep y ~800-1700 empty.
- No text in the art. One idea per piece.

**Light/dark**: every piece ships as `<name>_light.svg` and `<name>_dark.svg`, same geometry,
palette swapped. Light versions are pale and low-contrast (like a pencil study), dark ones
are the rich version.

**Size**: under 60 KB per file (`scenes.py` warns). Merge repeated shapes into one `<path>`
per fill (see `stone_wall`), skip geometry hidden behind the subject, and keep stipple
density low.

## The pieces

| file | used in | concept |
|---|---|---|
| `welcome_vault_*` | Welcome (hero, animated: `WelcomeArt`) | A round vault door in a stone wall, its face ringed with dial ticks. Five keyholes (five members) around a five-spoke hand wheel with verdigris grips; three gold keys are turned and a gold arc joins them: 3 of 5 opens it. Coins on the floor. |
| `create_stones_*` | New vault (banner) | A ring of standing stones at moonrise; the lit stones (two uprights and a lintel) form the arch, the others stay dark: enough members complete it. |
| `join_doorway_*` | Join a vault (banner) | An arched doorway with warm light spilling out, a verdigris keystone and door leaf, and a sealed invitation waiting on the step. |
| `key_shards_*` | Setup's key screen (hero, animated: `KeyShardsArt`) | One ornate gold key broken into five pieces, each with a member's verdigris gem, held apart around a dashed outline of the whole key: the full key never exists in one place. |
| `backup_seal_*` | Backup prompt after setup (banner) | An envelope in front of a large dial, the key share inside (dashed gold ghost), sealed by a combination lock with a verdigris dial; blank word cards (the passphrase) are threaded to it. |
| `restore_key_*` | Restore vault (banner) | The envelope open and its lock unlocked; the gold key flies along a dotted arc back to a safe's round door, whose keyhole glows verdigris. |
| `sent_slot_*` | Sending screen (full-page background, animated: `SentArt`) | A vault door bleeding off the top edge; light pours from its slot, flanked by verdigris member marks, and coins drift out to the edges along dotted trails. Faint dial ripples rise from the bottom. The middle is empty for the status circle and text. |
| `empty_ledger_*` | Activity, empty state (small banner, 1080 x 440) | A quiet shelf: a shut little safe, an open blank ledger with a verdigris ribbon, one coin. |

## In the app

`app/lib/src/features/onboarding/onboarding_art.dart` picks `<name>_dark.svg` or
`<name>_light.svg` from the active `AppColors`:

- `OnboardingHero('name')`: a `Positioned` for a full-screen `Stack(fit: StackFit.expand)`,
  placed first (behind the content). The art is full width at 1080:1240, with a gradient in
  the window colour: 85 % at the top (status bar legibility) → clear at 10 % → clear at
  40 % → solid at 74 %. Content goes below/over the faded part (Welcome pushes its title
  down with a `Spacer`). Pass `art:` for an animated scene (`WelcomeArt`, `KeyShardsArt`).
- `OnboardingBanner('name')`: 1080:560 `AspectRatio`, `BoxFit.cover`, clipped to
  `AppRadii.large`; use it as the first child of a `ZafeScreen`, followed by
  `SizedBox(height: AppSpacing.md)`. `OnboardingBanner('name', aspectRatio: 1080 / 440)`
  for the small banners (`empty_ledger`, inside the Activity empty card).
- `SentArt`: the sending screen's full-page art, passed as
  `MobileTransactionProgressScreen(background: ...)`.

Animated scenes are stacks of layer SVGs (`LAYERS` in `scenes.py`); how they move:
`docs/app-notes.md` "Animated art".

All are wrapped in `ExcludeSemantics` (decorative).

## Recipe: a new illustration

1. Write the concept in one sentence (what Zafe idea does it show?) and pick hero or banner.
2. Copy `scripts/illustrations/scene_template.py`'s function into `scenes.py`, rename it
   and add it to `SCENES` (the key becomes the file name).
3. Iterate: `scripts/illustrations/preview.sh <name>` regenerates the SVGs and renders
   them with flutter_svg into `app/build/illustration_preview/` (`<name>_light.png`,
   `<name>_dark.png`, `<name>_*_hero.png` for heroes with the in-app fade,
   `<name>_*_screen.png` for full-page backgrounds with a stand-in status circle,
   `contact_sheet.png` for everything, four per row). Look at both themes.
4. Use it: `OnboardingHero('<name>')` or `OnboardingBanner('<name>')` (assets in
   `app/assets/illustrations/` are already registered in `pubspec.yaml`).
5. Check the real app: `scripts/illustrations/device-shots.sh` (builds the x64 debug APK,
   installs it, **clears the app's data**, screenshots Welcome / New vault / Join in light
   and dark into `app/build/illustration_device/`). Needs a running emulator and
   `agent-device`. Add steps there if the new art is on another screen.
6. `cd app && flutter analyze lib test`; keep every SVG under 60 KB.

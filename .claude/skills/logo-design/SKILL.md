---
name: logo-design
description: Design or redesign an app logo / app icon / brand mark from the brief and the category, the way an identity designer would, and test it on every platform surface. Use when asked for a logo, app icon, launcher icon, brand mark, wordmark or "the logo for <app>", including styling it per accent colour. Produces a researched brief, a competitor mark audit, several concepts drawn as flat geometry in each style, scored against identity criteria, and a test bench artifact (iOS default/dark/tinted, Android adaptive/themed, notification silhouette, 16 px, splash, lockup).
---

# Logo design

How Zafe's Seam mark was chosen (docs/brand.md §5). "Icon" can mean the app logo or the
UI icon set: ask or infer from context before starting (it was misread once).

## 1. Brief → attributes → metaphors

Reuse the palette brief (`brand-palette` skill) if there is one. List 3-5 attributes, then
a metaphor map for each (Zafe: members coming together, two parts that lock, a door seam,
something hidden in plain sight). Write down the tropes to avoid for the category
(crypto/security: shields, padlocks, keyholes, keys, a letter in a circle = a coin,
hexagons, gradients, mascots; the ecosystem colour inside the mark reads as a token).

## 2. Research (delegate, keep working)

Spawn a general-purpose agent for: the identity process and criteria (Chermayeff &
Geismar & Haviv: appropriate, distinctive, simple; Rand: a logo identifies, it doesn't
explain; Bierut: an empty vessel), logo types (lettermark, symbol, combination, dynamic
identity), a mark audit of 10-20 neighbours (describe shape, style, colour; mark
unverified), platform rules, and metaphors used well. Meanwhile draw concepts.
Never redraw competitors' logos in the deliverable: describe them in words.

## 3. Draw concepts as geometry

Use `.claude/skills/logo-design/marks.js` (100-unit tile; `rounded()` polygons with
per-vertex radii, `offsets()` for mitred channels, `seam()`, `zQuorum()`, `zSolid()`).
Flat colours and plain paths only (flutter_svg: no `<line>`, `<polyline>`, patterns or
filters), so the numbers port straight to `scripts/brand/brand.py`.
- Sketch several, proof immediately: `node marks.js proof out.html`, screenshot it with
  agent-browser and look at 200/48/24/16 px plus the silhouette. Kill concepts that fail
  (Zafe's Dial read as a stopwatch and a Z-in-circle coin; a first Seam cut the block into
  three stripes like a hamburger icon because the channel bars ran full width).
- Keep 3 concepts including the current mark as the baseline.

## 4. Style per accent

Draw each concept in each palette's style (from the palette study): corners (sharp vs
large radii), weight, two-tone (upper/lower piece in two tones of the brand), an engraved
inline (clip the path to itself; stroke it wide in the tile colour, then narrower in the
mark colour) shown **only** at ≥96 px (optical sizing). Tiles use the deep brand colour.

## 5. Score and test

Score each concept 1-3 on appropriate, distinctive, simple, one colour, at 24 px. Then a
test bench for any concept × style:
- Home screen at 60 px among placeholder tiles (never other brands' icons).
- iOS 26 Default, Dark, Tinted (derive from Default/Dark/Mono layers).
- Android adaptive (circle mask, mark inside the 66 dp safe circle of the 108 dp canvas;
  the 100-unit tile maps to the visible 72 dp) and themed monochrome.
- Notification: 24 dp alpha-only silhouette in a status bar and a notification card.
- 16 px browser tab, splash (icon centred on the window colour), wordmark lockup (the
  app's display face), and any dynamic element (e.g. per-vault emblems).

## 6. Publish, decide, record

Load `artifact-design`; publish the study as its own artifact (brief, audit table,
concept × style matrix, test bench with concept/style pickers, recommendation). Screenshot
desktop and phone width before reporting (overflowing mock status bars and clipped chips
were caught this way). Record the chosen geometry (exact numbers) and the decision in
docs/brand.md; the implementation goes into `scripts/brand/brand.py` +
`scripts/brand/icons.sh` (see `docs/app-notes.md` "App icon and splash").
Before recommending a dynamic element, check what landed in the app meanwhile (Zafe's
per-vault picture emblems replaced the "2/3" avatar during this study; the N-of-M ring was
deferred rather than overwriting it).

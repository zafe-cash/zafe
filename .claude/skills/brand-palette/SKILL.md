---
name: brand-palette
description: Design or revise an app's colour palette and icon style from its audience and category, the way a brand/UI designer would. Use when asked to pick, compare or fix accent colours, theme colours, "the palette looks awkward", light/dark theme colours, semantic (success/warning/error) colours, or an icon style that matches a brand. Produces a researched brief, a competitor colour map, OKLCH-derived palettes with contrast checks, per-palette icon styles, and a mockup artifact of the real screen.
---

# Brand palette

How Zafe's palette was chosen (docs/brand.md). Follow the steps in order; each one feeds
the next. The user reviews options in a published artifact, so the deliverable is a page,
not a paragraph.

## 1. Read what exists first

- Theme code (`app/lib/src/core/theme/primitives.dart`, `colors/*`), hard-coded colours in
  signature widgets (grep `Color(0x` under `lib/`), the illustration palette
  (`docs/illustrations.md`), the icon set (`app/assets/icons/`), and `docs/brand.md`.
- Measure the current colours in OKLCH: `node .claude/skills/brand-palette/oklch.js hue
  '#C9EE6E' ...`. Existing neutrals often already carry a hue tint; keep their lightness steps.
- Mock **today's** screen before proposing anything: every option must be judged on the
  real layout (Home: top nav, vault card, buttons, notice card, activity rows, tab bar).

## 2. Write the brief

Who uses it, the job they do, 3-5 attributes it must communicate, and what that rules out.
Get the audience from `spec.md` and memory (Zafe: DAO/grant committees co-signing; calm,
collective, private, part of Zcash; not a trading app, not a "treasury").

## 3. Audit the category (delegate to a research agent)

Spawn a general-purpose agent to collect brand hex values of 10-20 nearby products
(direct competitors, the ecosystem, adjacent fintech, privacy apps) with sources, marking
anything unverified, plus the professional methods in step 4. **Keep working while it
runs** (mock the current UI, draft code); don't idle waiting for it.
Then plot the hues on the 0-360 OKLCH circle, shade the crowded bands and name the open
ones. Flag near-collisions with ΔE in OKLab (Zafe's lime was ΔE 0.04 from MetaMask's).

## 4. Derive palettes, don't pick hexes

Rules (Stripe accessible colour systems, Linear LCH themes, Material 3 HCT, Radix, Atlassian):
- **Separate jobs**: brand (actions, "needs you"), value (for Zafe: Zcash gold = money,
  keys), status (warning, error). One colour never means two things.
- **Lightness per role**, fixed in OKLCH, so every hue passes WCAG AA by construction.
  Grays = the app's ladder lightnesses with ~1% of the brand chroma.
- **Natural value per hue**: yellow/lime/orange peak chroma when light (dark labels in both
  themes); blue/violet/teal/green peak when darker (deep fills, white labels in light theme).
- **Status hues ≥35° from the brand and from the value colour.** Check all pairs: a
  warning amber at 80° next to gold at 83° slipped into one round.
- Generate with `oklch.js derive <hue> [chroma fillL deepL deep|tint] --pos <h|value>
  --warn <h> --bad <h>`; it prints tokens and every text pair's contrast (FAIL < 4.5).
- Offer 4-6 options, each tied to a meaning from the brief and to open space in the audit.
  Always include the current palette unchanged as the baseline, and list dropped
  directions with the reason (usually: a crowded hue).

## 5. Give each option an icon style

Phosphor's rule: one default weight, filled only for state (active tab, "needs you").
Stroke weight follows the body font; corners follow the UI radii. Make the styles
**visibly** different: 1.4 vs 1.6 vs 1.75 px strokes look identical at 18 px. Vary
several things at once: weight (1.25-2.3 px), joins (round vs mitre), fill (outline,
two-tone with the accent at ~35%, solid), and tile shape (square, circle, outlined ring,
squircle). Show a 26 px specimen per option (idle and active) besides the in-mockup icons.

## 6. Build the comparison artifact

Load the `artifact-design` skill. One page: the brief, the category map, the rules, then
a grid of phone mockups of the real screen (one per palette) with a dark/light toggle,
a hue strip (brand, value, status marks), swatches and the contrast ratios under each.
Hero surfaces that stay dark in light mode (Zafe's vault card) get their own token set
and a toggle to compare treatments. The user decides; record the decision in docs/brand.md.

## 7. Verify by looking

Screenshot the page with agent-browser (`open file://...`, `screenshot`, read the PNG) in
dark and light and with every toggle, before telling the user it's done. A node run of the
script is not enough: a stray half-rule left by a CSS edit voided the next rule
(`position:absolute` on the vault-card dial) and it only showed on screen. Check phone
width (no horizontal scroll) too.

## 8. Hand-off

Write the research, tokens and decisions to `docs/brand.md` (sources, unverified marks,
what was deferred), and update the palette notes in `docs/app-notes.md` when the app changes.

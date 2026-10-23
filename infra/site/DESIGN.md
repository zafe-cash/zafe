---
name: Zafe website
description: The Zcash multisig nobody can see. One shielded vault for your group; nothing moves until enough of you approve.
colors:
  paper: "#F1F5F5"
  ground: "#FFFFFF"
  raised: "#E5EAE9"
  line: "#D9DFDF"
  ink: "#090E0E"
  text-secondary: "#4B5755"
  verdigris: "#00736C"
  verdigris-pressed: "#005F59"
  brand-label: "#FFFFFF"
  band: "#E6ECEB"
  vault-card: "#E0F3F1"
  seam-tile: "#004A46"
  footer-ink: "#0A0E0E"
  footer-text: "#F1F5F5"
  footer-muted: "#9DA9A7"
  ghost-wordmark: "#161C1C"
  selection: "#BFE8E4"
  placeholder: "#66726F"
  scene-ink: "#1E2B2A"
  scene-wall: "#9DC5BD"
typography:
  display:
    fontFamily: "Space Grotesk, DM Sans, system-ui, sans-serif"
    fontSize: "clamp(44px, 6vw, 92px)"
    fontWeight: 500
    lineHeight: 1.02
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Space Grotesk, DM Sans, system-ui, sans-serif"
    fontSize: "clamp(38px, 5vw, 64px)"
    fontWeight: 500
    lineHeight: 1.02
    letterSpacing: "-0.035em"
  title:
    fontFamily: "Space Grotesk, DM Sans, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: "-0.025em"
  title-small:
    fontFamily: "Space Grotesk, DM Sans, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.015em"
  lead:
    fontFamily: "DM Sans, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1.55
  body:
    fontFamily: "DM Sans, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: "DM Sans, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.55
  action:
    fontFamily: "DM Sans, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 500
    lineHeight: 1.55
  code:
    fontFamily: "JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
rounded:
  footer-lip: "48px"
  loop: "32px"
  loop-mobile: "24px"
  card: "28px"
  contact: "20px"
  button: "14px"
  input: "12px"
  nav-button: "10px"
  pill: "999px"
spacing:
  step: "160px"
  step-mobile: "112px"
  band-step: "160px"
  band-step-mobile: "72px"
  loop-gap: "48px"
  loop-gap-mobile: "32px"
  gutter: "24px"
  gutter-loop-mobile: "12px"
  column-hero: "60rem"
  column-loop: "100rem"
  column-reading: "44rem"
components:
  button-get-zafe:
    backgroundColor: "{colors.verdigris}"
    textColor: "{colors.brand-label}"
    typography: "{typography.action}"
    rounded: "{rounded.button}"
    padding: "0 40px"
    height: "56px"
  button-get-zafe-hover:
    backgroundColor: "{colors.verdigris-pressed}"
  button-play:
    backgroundColor: "{colors.verdigris}"
    textColor: "{colors.brand-label}"
    typography: "{typography.action}"
    rounded: "{rounded.button}"
    padding: "0 26px 0 22px"
    height: "56px"
  button-iphone:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.action}"
    rounded: "{rounded.button}"
    padding: "0 24px 0 20px"
    height: "56px"
  button-iphone-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ground}"
  hero-badge:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "4px 18px 4px 8px"
    height: "44px"
  hero-badge-chip:
    backgroundColor: "{colors.vault-card}"
    textColor: "{colors.verdigris}"
    rounded: "{rounded.pill}"
    padding: "3px 10px"
  nav-get:
    backgroundColor: "{colors.verdigris}"
    textColor: "{colors.brand-label}"
    rounded: "{rounded.nav-button}"
    padding: "8px 16px"
  tab-icon:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.input}"
    size: "48px"
  tab-icon-hover:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
  platform-tab:
    backgroundColor: "{colors.paper}"
    padding: "2px 28px 12px"
  loop-frame:
    backgroundColor: "{colors.paper}"
    rounded: "{rounded.loop}"
  loop-toggle:
    backgroundColor: "rgba(255, 255, 255, 0.85)"
    textColor: "{colors.ink}"
    rounded: "{rounded.pill}"
    size: "40px"
  waitlist-input:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.input}"
    padding: "0 16px"
    height: "52px"
  waitlist-button:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.input}"
    padding: "0 22px"
    height: "52px"
  waitlist-button-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ground}"
  popover:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
    padding: "32px 32px 20px"
  play-block:
    backgroundColor: "{colors.vault-card}"
    rounded: "{rounded.card}"
    padding: "36px 36px 40px"
  contact-card:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.contact}"
    padding: "24px 20px"
---

# Design System: Zafe website

Scope: the home page (`/`), `/download`, `/support`, `/privacy`, `/waitlist`, `/waitlist-error`, the 404, and the shared nav, footer, install buttons, popovers and waitlist form. Out of scope: `/showcase` keeps its own world (the 3D diorama, its light fixed nav, title card and slim footer) and is not governed by this file. Brand source: `docs/brand.md` (Verdigris); illustration pipeline: `docs/site.md` §6.

## Overview

**Creative North Star: "One Room, Real Cards"**

The page is a calm paper column, and its pictures are drawings of one room: a desk in shade, a window patch of warm light, a lamp, an envelope and its wax, a brass tube, a strongbox, a sleeping cat. Each picture is a short loop in which the scene moves around one card the app renders itself (the request, the send, the keys, the vault). The drawing is authored, not generated: Blender scenes shaded in three flat toon bands from a hand-picked colour script, outlined with tinted, tapered Line Art ink, with a paper grain laid over the scene but masked off the card so the UI stays crisp.

The page around the drawings is quiet: a centred hero that fills the first screen (a Google Play status badge, the H1, a sub-line, one "Get Zafe" button), then a sage band whose paper tab holds the platform icons and the first loop, then four story steps, each an H2, one line and a loop. Space Grotesk headings at 500, DM Sans prose in secondary ink, 160px between steps. Verdigris marks only actions.

Motion is the loops themselves, a CSS-only intro that collapses the mark into the nav logo, and the hero rising in as it hands over. The page has one script, `loops.js`: it shows each loop's pause button and pauses loops that are off screen. With reduced motion there is no intro, no rise, and every loop is a still.

**Key Characteristics:**
- Light scheme only (`color-scheme: light`); the shared footer is the one dark band.
- Imagery is illustration: one room, toon bands, tinted ink lines, the app's own component cards on top. No screenshots in phone frames, no 3D renders, no stock.
- Verdigris means "act". Nothing else on the page is coloured; the colour lives in the drawings.
- One script, no inline scripts or styles (CSP `script-src 'self'; style-src 'self'`); popovers and forms work without it.
- Every moving thing has a still: reduced motion shows stills, and loops pause.

## Colors

A cool, nearly neutral paper palette tinted toward verdigris, with one teal for action; the warm, saturated colour belongs to the illustrations.

### Primary
- **Verdigris** (#00736C): the Get Zafe button, the nav's Get Zafe button, the Google Play button, the FAQ disclosure markers on the home page, focus rings, the email caret, links in the privacy policy.
- **Verdigris Pressed** (#005F59): hover on every verdigris button (fine pointers only).

### Neutral
- **Paper** (#F1F5F5): the page ground, the platform tab, and the loop frame behind each video.
- **Sage Band** (#E6ECEB): a shade below paper; the band that holds the platform tab and the first story step.
- **Ground White** (#FFFFFF): the hero badge, popovers, support contact cards, form fields, code blocks on /download, the pause button (at 85%).
- **Vault-Card Mint** (#E0F3F1): the "Beta" chip in the hero badge and the Google Play block on /download (the app's light vault card).
- **Raised Grey** (#E5EAE9): tab icon hover.
- **Hairline** (#D9DFDF): field borders and the hero badge's border.
- **Ink** (#090E0E): headings, body text, outline buttons and their hover fill.
- **Secondary Ink** (#4B5755): sub-lines, step lines, captions, nav links, feature icons, notes.
- **Placeholder** (#66726F), **Selection Mint** (#BFE8E4).
- **Seam Tile** (#004A46): the intro's full-screen veil, the colour of the mark's tile.
- **Footer Ink** (#0A0E0E), **Footer Text** (#F1F5F5), **Footer Muted** (#9DA9A7), **Ghost Wordmark** (#161C1C): the footer band only.

### Illustration
- **Scene Ink** (#1E2B2A): the card's 1.6px outline in every loop and the base of the Line Art tint.
- **Scene Wall** (#9DC5BD): the room's mid wall tone, the field behind the scene. The full colour script (shadow, mid, light per material: wall, desk, paper, envelope, verdigris, brass, gold, wood, terracotta, leaf...) lives in `infra/site/scenes/props.py`; shadows lean cool, lights warm.

### Named Rules
**The Teal Is Action Rule.** On the page itself verdigris marks something you can press (or the FAQ marker). No teal headings, tints or decorative fills.

**The Colour Lives In The Room Rule.** Warm and saturated colour appears only inside the illustrations, from the colour script. The page chrome stays paper, white, ink and teal.

## Typography

**Display Font:** Space Grotesk 500/600 (with DM Sans, system-ui)
**Body Font:** DM Sans 400/500 (with system-ui, -apple-system, Segoe UI, Roboto)
**Mono Font:** JetBrains Mono (with ui-monospace, SFMono-Regular, Menlo)

**Character:** The app's own fonts, so the page and the cards in the loops speak with one voice. Space Grotesk large and tight (500, -0.035em, balanced wrap) for headings; DM Sans for everything a person reads.

### Hierarchy
- **Display** (500, clamp(44px, 6vw, 92px), 1.02): the hero H1 only, max 10.5em, 24px below; clamp(42px, 12vw, 56px) under 600px. Other page H1s: /download clamp(44px, 6vw, 76px), /support clamp(40px, 6vw, 72px), 404 and waitlist pages clamp(40px, 5vw, 64px), privacy clamp(36px, 5vw, 56px).
- **Headline** (500, clamp(38px, 5vw, 64px), 1.02): the story H2s, "Get Zafe", "Questions".
- **Title** (500, 28px, 1.15): /download option headings. Support's "Quick fixes" is clamp(28px, 3.4vw, 40px).
- **Title small** (600, 20px): popover titles and support contact labels.
- **Lead** (400, 18px): the hero sub-line (max 24em), each step's line, the close line; secondary ink.
- **Body** (400, 17px, 1.55): base text, FAQ answers, feature lines. FAQ questions 18px at 500.
- **Label** (400, 15px): loop captions, notes; the hero badge at 500 (its chip 13px); 14px for the waitlist note.
- **Action** (500, 17px): button labels.
- **Code** (400 JetBrains Mono, 14px): the checksum command and certificate on /download only.

### Named Rules
**The Sentence Case Rule.** No uppercase tracked labels, eyebrows or kickers above headings. A step starts with its H2. The hero's Google Play badge is a link with a real status, not a label, and the only one. Like every link to Google Play on the site it is a `PlayButton`: a QR popover on a computer (hover + fine pointer), the Play listing directly on a phone.

**The One Line Rule.** A story step is a heading and one sentence; the loop carries the rest (its moments are repeated as a screen-reader list, not as visible copy).

## Layout

A centred column, everything text-aligned centre on the home page. The hero fills most of the first screen: min-height min(calc(100svh - 80px - 62px - 72px), 700px) (calc(100svh - 68px - 62px - 48px) under 600px), content centred vertically by flex in a 60rem column (badge, 28px, H1, 24px, sub-line, 32px, button), so the band's 62px platform tab sits a little above the bottom edge of the first screen as the cue to scroll.

The band (sage) spans the full width; its paper tab hangs into it from the hero, centred, with 28px inverted fillets either side so the paper seems to flow down into it. The first story step sits in the band (160px down, 124px on phones), so the first screen ends on the tab. The other steps follow on paper, 160px apart (112px under 600px). Each step: H2, the line 16px below, then the loop 48px below (32px on phones) in a column up to 100rem with 24px gutters (12px on phones). Loops are 16:9; on phones (700px and below) and portrait tablets (portrait, up to 1100px) they switch to a 9:16 cut, never taller than 88svh.

"Quiet until it needs you" ends with a feature list 64px below its loop: icon-and-line items in three columns (70rem), two at 1100px, one under 700px. The close ("Get Zafe", one line, Google Play and iPhone side by side) sits 152px down; the FAQ (44rem, centred heading) 160px below that; 72px to the footer.

/download and /privacy are a single 44rem reading column, /support 46rem centred; options 72px apart. Under 600px install buttons go full width and the waitlist row stacks.

## Elevation & Depth

Flat and tonal: paper, sage band and white surfaces carry structure. Shadow is long, low and tinted toward verdigris, and appears only on things that float: the loop frames, popovers, and support cards on hover. Inside the illustrations depth comes from the room's three toon bands and the card's flat cast shadow.

### Shadow Vocabulary
- **Loop frame** (`box-shadow: 0 30px 60px -45px rgba(0, 40, 38, 0.35)`): every loop frame.
- **Popover** (`box-shadow: 0 40px 80px -30px rgba(0, 40, 38, 0.45)`, backdrop rgba(9, 14, 14, 0.35)): the Google Play QR and iPhone waitlist popovers.
- **Contact hover** (`box-shadow: 0 18px 40px -28px rgba(0, 40, 38, 0.45)`): support contact cards, fine-pointer hover only.
- **Card cast shadow** (`16px 18px 0 -2px rgba(30, 60, 56, 0.28)`, 12px 14px in the tall cut): the app card inside each loop, drawn into the video. A flat, translucent shadow in the scene's own language (toon bands have no soft gradients); it belongs to the drawing, not to page elements.

### Named Rules
**The Drawn Shadow Rule.** The flat offset shadow and ink outline are the illustration's devices for the card it holds. Page HTML (buttons, cards, fields, popovers) never gets an offset shadow or an ink outline.

## Shapes

Soft rectangles that echo the app: 32px for loop frames (24px in the tall cut), 28px for popovers, the /download Play block and the platform tab's lower corners, 20px for support contact cards, 14px for install buttons, 12px for inputs, tab icons and code blocks, 10px for the nav's Get Zafe, full circles for the pause button. The footer opens under a 48px rounded paper lip. The loop's app card is drawn at the app's own 28px card radius (7.8% of its width), outlined in scene ink.

## Components

### Buttons
Quiet, solid, pressable.
- **Shape:** 14px corners, 56px tall.
- **Get Zafe (hero):** verdigris, white label, 0 40px padding; links to /download. The hero's only button.
- **Google Play:** verdigris with the Play glyph (20px) leading, 0 26px 0 22px. On touch it opens the listing; with a fine pointer it opens a popover with a QR code drawn at build time.
- **iPhone waitlist:** outline (1px ink, transparent), App Store glyph (18px); opens the waitlist in a popover. Hover inverts to ink.
- **Hover / Focus / Press:** hover only behind `(hover: hover) and (pointer: fine)`, 0.2s. Focus is a 3px verdigris outline, 2px offset. Press scales to 0.97 over 160ms cubic-bezier(0.23, 1, 0.32, 1) (0.94 for icon buttons).
- **Text link (/download):** secondary ink, 44px target, ink on hover.

### Hero Badge
The one badge, above the H1: a white pill (1px hairline, 44px tall, 4px 18px 4px 8px) linking to Google Play (QR on a computer): a mint "Beta" chip in verdigris (13px), "Now on Google Play" (15px, 500), then the Play glyph (16px, secondary ink). Border turns verdigris on fine-pointer hover; 0.97 press. The audit caveat stays in the FAQ and on /download.

### Platform Tab (signature)
A paper tab hanging from the hero into the sage band: two 48px icon buttons (Google Play, App Store glyph, 22px, secondary ink; hover raised grey and ink), with inverted 28px fillets joining it to the hero's paper.

### Story Loop (signature)
A figure: the video in a paper frame (32px, loop-frame shadow), poster first, `preload=metadata` except the first. A paper grain (20%, multiply) lies over the frame, masked off the card by a per-loop SVG mask. A 40px round pause button sits bottom-right (white 85%, ink glyph), hidden until `loops.js` runs. On phones and portrait tablets the tall cut, its still and its mask swap in. With `prefers-reduced-motion: reduce` the video and button are hidden and a still (WebP, with alt text) shows. The moment's beats are an `ol` for screen readers. An optional caption (15px, secondary) sits 20px below.

### Feature List
The feature list: plain lines, each led by the app's Patina two-tone icon (22px, secondary ink), no tiles, no borders.

### Inputs / Fields
- **Style:** the iPhone waitlist: white field, 1px hairline, 12px corners, 52px tall, 16px padding, verdigris caret. A plain form posting to `/api/waitlist`; no script.
- **Submit:** "Notify me", outline (1px ink, white), 12px corners; inverts to ink on hover; 0.97 press.
- **Focus:** 3px rgba(0, 115, 108, 0.35) outline and verdigris border.
- **Note:** 14px secondary line with an underlined Privacy link.

### Popovers
Native `popover` (no script): white, 28px corners, 32px 32px 20px padding, popover shadow, fades and scales from 0.96 over 200ms (`@starting-style`). Title in Space Grotesk 20px 600; links row in 15px secondary ink with 44px targets.

### Navigation
Links either side of the Seam mark, which sits exactly centred (44px). Home: "How it works", "FAQ" left; "Support" right, then Get Zafe as a small verdigris button (10px corners, 8px 16px). Elsewhere a back chevron "Home". 16px secondary ink, ink on hover with a 1.5px underline that grows from scaleX(0) over 0.25s. On phones the mark moves left and only links marked mobile stay.

### Disclosure (FAQ, checksum)
Native `details`, the first FAQ item open. The marker is the app's Patina plus as a 16px mask (verdigris on the home FAQ, secondary ink on /download), rotating 45deg to an x over 200ms. 44px summary, 18px question; answers indent 32px in secondary ink. No dividers.

### Footer
The one dark band: footer ink under a 48px paper lip; the mark with "Privacy, held together." (Space Grotesk 30px), three link columns (get it, the project, help; brand glyphs at 15px, a small "soon" pill), a bottom row ("No cookies. No analytics.", X and GitHub at 44px targets, "Zafe, 2026"), and a giant "zafe" wordmark in ghost ink cut by the page's bottom edge.

### Intro
CSS only, home page: the screen is the mark's tile; its two pieces lock, the veil closes in a circle onto the nav logo (0.8s, cubic-bezier(0.7, 0, 0.2, 1)), two verdigris rings pulse out, and the hero rises in (0.8s, cubic-bezier(0.2, 0.8, 0.2, 1), staggered 1.45-1.72s; the band at 1.8s). Removed entirely with reduced motion.

## Do's and Don'ts

### Do:
- **Do** picture every moment as one room drawn in toon bands and tinted ink, with one card the app renders itself on top (components, not cropped screens).
- **Do** keep the paper grain a page layer masked off the card, never baked into the video.
- **Do** give every loop a pause button and a reduced-motion still, and keep `loops.js` the page's only script.
- **Do** keep pages free of inline scripts and styles (CSP); popovers and forms must work without JavaScript.
- **Do** keep each story step to an H2, one line and a loop, 160px apart (112px under 600px).
- **Do** keep verdigris for actions (The Teal Is Action Rule).
- **Do** gate hover behind `(hover: hover) and (pointer: fine)` and press with a 0.97 scale over 160ms cubic-bezier(0.23, 1, 0.32, 1).
- **Do** keep targets at least 44px.

### Don't:
- **Don't** put eyebrow labels, kickers or badges above headings; the hero's Google Play badge is the single exception, not a pattern.
- **Don't** number the steps; the headings carry the order.
- **Don't** use phone-frame screenshots, 3D renders, generated scenes or stock imagery on these pages.
- **Don't** use smooth gradient shading or Freestyle lines in the scenes; three flat bands and Line Art only.
- **Don't** put an offset shadow or ink outline on page HTML (The Drawn Shadow Rule).
- **Don't** build icon-card grids, facts strips, comparison tables or bento layouts.
- **Don't** add a dark mode or dark sections beyond the shared footer.
- **Don't** bring the /showcase world into these pages.

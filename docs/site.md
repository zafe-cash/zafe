# Zafe landing page: design research and brief

The landing page (`infra/site/src/pages/index.astro`) follows this brief. Keep it current
when the page changes.

## Round 12d: the minor items too

"Fix the minor issues too, also update the skill accordingly." Fixed the three 12c left:
"Single-key" sits in a `.nowrap` span (the font subset has no U+2011, so a non-breaking
hyphen would fall back to another font); the comparison got its `Compare` tag like every
other section; on phones the wide 3D shots are pulled back ×1.42 instead of ×1.55
(`buildCamera`), so the diorama fills the width (×1.3 cut the edge phones). The skill now
says minor issues get fixed in the round that finds them, and the root README is current
(versioned formats, phone features, website, layout, links).

## Round 12c (critique pass): nothing above minor, loop stopped

Checked both pages again at 1440×900 and 390×844 (showcase with the world, its stills
fallback and no-JS), plus nav clicks live. Only minor items left: "Single-key wallet"
breaks at its hyphen on phones (the font subset has no non-breaking hyphen); the
comparison is the one section without a tag; the world's mid-story framings are small on
phones (world.js, untouched on purpose).

## Round 12b (critique pass): the hero phones, the share image, phone details

Found (ranked): at 1200 px the two hero phones stood side by side (the back one was
placed from the left edge, so the overlap changed with the width: a rejected look); the
share image `og.png` still showed the old showcase hero; on phones the showcase teaser
had a tall empty band and the comparison's labels wrapped to four lines; competitors'
"Yes" was teal, the colour kept for Zafe and actions. Fixed: the back phone hangs off
the front one (`--ph`, the front phone's height; back at `right: 4% + 0.29 * --ph`,
tilted -4°), so they overlap the same at every width; `og.png` is now the home hero at
1200×630 (`stills.sh` captures it from `/` after the intro); teaser 440 px on phones;
comparison label column 40% wide, competitors' "Yes" in text colour.

## Round 12a (critique pass): spacing and the showcase entrance

Found (ranked): the showcase's scroll cue sat on the right phone's pedestal; on phones
the showcase title hugged the nav with a ~250 px hole above the world; ~170 px of dead
paper between the home call to action and the footer; the cue was 28 px off centre
(`.home p { margin: 0 }` beat its `margin-left`). Fixed: the cue sits bottom centre
(`left: calc(50% - 28px)`, measured at 720 of 1440); the phone title is centred in the
space above the world; the call to action ends the page with only the footer's lip
below. Minor, left: `og.png` still has the old showcase copy (needs Pillow for `stills.sh`).

## Decisions (2026-10-01, twelfth round): two pages that don't look alike

The user: "their is no showcase in the top nav, the home doesn't take me to / route, same
with clicking logo. we have mostly used the same background with stills on /, on
showcase it's the replica of the main site."
- **The nav "bug"**: clicks were never blocked. Reproduced in headless Chrome, locally
  and on the live site, world on and off, scrolled, custom cursor on: Home and the logo
  always reached `/`, and Lenis's `anchors` only handles same-page hashes. What made it
  look broken: both pages replayed the same full-screen logo intro and opened on the same
  render of the vault, so arriving on `/` looked like a reload of `/showcase`; on phones
  the left links were hidden, so the showcase had no Home link at all; and the nav
  scrolled away, so the 1100vh story had no way out.
- **Nav** (`Nav.astro`): home has How it works, Security, **Showcase**; links marked
  `mobile` stay on phones (the mark moves left, the marked links sit right). The
  showcase gets a `light` nav: fixed, small, "‹ Home", the mark, Get Zafe.
- **Home shows the product, not the world**: hero = two real phones (your light home
  screen in front, Bob's dark review behind) over a faint dial; "How it works" = one
  payment in three large phone cards, alternating sides (propose, check and approve,
  the sending screen); one wide still of the world as the door to /showcase; features =
  the review screen's checks ("Matches", Approve and sign), the real testnet tx card, a
  privacy list; security = heading and the testnet warning on the left, four rows on
  the right (no pillar columns; the fourth, "No telemetry", added 2026-10-07); the call to action on the vault card colour with Bob's
  home screen. One still left on the page (the teaser), was nine.
- **Showcase is its own experience**: no logo intro (the world fades up from the paper),
  a title card bottom left ("Zafe, in 3D" / "One payment, start to finish" / "Three
  phones share one vault. Scroll to play.") with a scroll cue instead of buttons, the
  islands' heading "What you didn't see" (was the home's feature heading), the ending
  "Your turn" with "Back to home", a one-line footer. The world and story are unchanged.
- `stills.sh` now opens `/showcase.html?still` (it still pointed at `/`, which no longer
  has the world). Stills weren't re-rendered: the world didn't change. `og.png` still
  shows the previous showcase hero copy; re-render it with `stills.sh` when Pillow is at
  hand.

## Decisions (2026-10-01, eleventh round): a calm home page, the 3D on /showcase

The user, mid-way through the fallback work: "for a multisig website having such
animations looks a bit odd", then "migrate the 3d animation to another page like showcase
or something, so that it doesn't mess with the main page. then build the website like
safe tailored to our rules here". Reasoning agreed on: treasurers judge trust (sober,
honest about security, fast); the 3D story still explains multisig best, so it stays,
one click away.
- **`/showcase`** (`showcase.astro`) is the 3D page as it was (world, story, islands,
  call to action from above, stills fallback), with its own hero ("One payment, start to
  finish"), no FAQ or JSON-LD, nav back home. In the sitemap.
- **`/` (index.astro, `body.home.landing`, `lp-*` classes)** follows safe.global's
  order, to our rules: hero (headline left over the rendered vault still, "Get Zafe" +
  "Watch it in 3D"); a facts strip instead of stats we don't have (shielded on Zcash,
  FROST, open source, Tor built in); how it works in three steps (beat stills); four
  feature cards (one tap, checked on every phone, invisible on-chain with the real tx,
  private all the way); security in four plain pillars, the last saying "testnet only,
  not audited"; a comparison (Zafe / on-chain multisig / single-key wallet, three
  factual rows); FAQ; the call to action over the view from above; a shared footer.
  No partner logos, testimonials or numbers: we have none.
- **No JavaScript** on the home page (the JSON-LD block is data); motion is the CSS logo
  intro and reveals on scroll timelines (none with reduced motion).
- Shared pieces became components: `Nav.astro` (left links per page), `Footer.astro`;
  `specUrl` lives in `config.ts`.
- `vercel-output.sh` now serves every top-level page at its clean path (it only knew
  `/join`; `/showcase` would have 404'd on Vercel).

## Decisions (2026-10-01, tenth round): the static fallback in the new design, and SEO

"Update the static fallback to match the new design." Visitors without the world (no
GPU, reduced motion, **no JavaScript**: Tor Browser "Safest", a real share of this
audience) saw the old dark/light phone split.
- **Stills of the world itself**: `infra/site/stills.sh` renders the built site in a
  capture mode (`/?still`: best quality, the camera lands at once, the page's text
  hidden; `?still=page` keeps it) into `public/assets/stills/*.webp`: the hero (wide,
  and tall for phones), five story beats, the three islands (rendered 1920 wide so the
  whole island fits, then cropped) and the view from above. Re-run it when the world or
  the story changes.
- **Same layout**: the hero is asymmetric over its still (the same rules as the world);
  the story becomes "How it works" with the five beats as rounded pictures, each caption
  pill on its picture's lower edge; the island cards carry their island on top, in three
  columns; the call to action sits over the view from above.
- **Never fetched by the world's visitors**: `public/assets/boot.js` (blocking, in the
  head) adds `html.js` before the first paint; `.still` shows only with
  `:is(html:not(.js), html.stills)`, and `story.js` adds `stills` when it picks the
  fallback (also on a load error or when the world gives up). Lazy images that stay
  hidden are never requested.
- **SEO** (side agent; `docs/seo.md`): canonical, Open Graph and Twitter tags,
  JSON-LD (Organization, WebSite, MobileApplication, FAQPage from the page's own FAQ
  list), sitemap and robots endpoints, favicons and manifest, title with search terms
  (the tagline stays the H1 and the share title). The social image `public/assets/og.png`
  is rendered by `stills.sh` too. The story got a heading ("How it works"; visually
  hidden over the world) and the chain card's labels reach 4.5:1 contrast.
- **Open**: the hero still appears only after the CSS intro (~1.5 s), which delays LCP;
  a custom domain (then change `site` in `astro.config.mjs`).

## Decisions (2026-10-01, ninth round): performance

The user asked to "optimize the website a bit for performance". Measured first (bundle
sizes, headers, renderer settings, rAF rate in headless Chrome).
- **Caching**: hashed bundles (`/assets/_astro/*`, world 179 KB + main 51 KB gzipped)
  were revalidated on every visit; now `max-age=31536000, immutable`. Fonts and screens:
  a week + `stale-while-revalidate`. Source: `public/_headers`; `vercel-output.sh` turns
  each `/assets/...` block into a route.
- **Adaptive quality** (world.js `LEVELS`): starts at device pixel ratio ≤ 2 with 4×
  MSAA (phones: 1.5, none, as before), averages frame time over ~1 s windows and steps down while
  under ~45 fps (pixel ratio, MSAA; shadow map 2048 → 1024 from level 2). Never steps
  back up (no flicker); ignores 2 s after start/resize and 1 s after the world or tab
  was hidden.
- **No GPU, no world**: a software WebGL renderer (SwiftShader, llvmpipe; hardware
  acceleration off) took ~4 s per frame and froze scrolling. `story.js` checks the
  renderer name and shows the stills without downloading the world; at runtime the
  world gives up (stops, frees the GPU, stills layout) if it stays under ~20 fps at its
  lowest level. `?world=always` overrides both for headless checks.
- **Screens load in parallel** with the world module (started in `story.js`), not after it.
- Checked, no change needed: three.js is tree-shaken (what's left is referenced by the
  renderer); backdrop blur on the cards isn't a measurable cost; fallback stills are lazy.
- **Open**: the stills fallback still uses the older dark/light phone split.

## Decisions (2026-10-01, eighth round): islands, the view from above, pointer and text

- **Features are islands in the same world** (the dark feature panel, the "idea" line
  and the closing card are gone). After "Sent." the camera follows floor channels on to
  three islands, one card each on the left (`.islands`, `data-island` set by the
  timeline): *Checked on every phone* (the review screen, a scan beam, badges),
  *Invisible on-chain* (glass blocks of identical tokens; a gold coin turns grey inside;
  the real testnet record on the card), *Private all the way* (arches lit by a passing
  packet, a sealed backup).
- **The call to action looks down on the vault**; a big Z lights in the floor around it
  and the card sits low so the mark reads above it. The world pauses only while the FAQ
  or footer covers the whole screen.
- **Custom cursor** (src/scripts/ui.js): the user didn't like a companion next to the
  system pointer ("the mouse is still the plain old pointer"), so on fine pointers
  without reduced motion the system pointer is replaced (`cursor: none`, added only once
  ours draws): a dark dot exactly on the pointer (white halo, readable on teal) and a
  trailing teal ring (a "Scroll" label that followed it over the scenes was removed at the
  user's request). Links swell the ring into a
  soft teal disc; on a "Get Zafe" pill the ring wraps it, the pill leans toward the
  pointer and the dot steps aside (the Seam-mark dot sat on the label). Touch devices
  keep their native behaviour. `?pointer=fine` forces it on for headless checks.
- **Text**: headings rise word by word from a mask; two "unshield" scrambles only (the
  proposal caption, the encrypted chain fields); an approvals counter (0, 1, 2 "of 2
  approvals") rolls as the shards seat. Custom splitter, not SplitText: no plugin, and
  it only sets transforms and text through the DOM, which the CSP allows.

## Decisions (2026-10-01, seventh round): the Seam Vault world

Feedback on round six: make the whole site canvas-like, bigger phones, the dark/light
split may not make sense, the lock should be a vault, the phones should hold key shards
that combine to open it; think like a 3D designer. Three research agents (design
choreography with 18 reference sites, WebGL implementation incl. a CSP probe, asset and
tooling routes) fed the plan; the user then allowed loading files (`connect-src 'self'`).

- **One world, no split.** An isometric diorama on grained paper; members' app themes
  stay (Bob dark under a warm lamp, you light in daylight). One fixed canvas behind the
  page (`canvas.world`, src/scripts/world.js); hero and story are transparent over it.
- **The vault door is the Seam mark**: its two pieces are the leaves (extruded from the
  SVG paths), the Z channel is the keyhole with gold light leaking through. **The key is
  the Z**, in two shards (top bar + upper diagonal; lower diagonal + bottom bar); each
  phone holds one, Cara's is the spare.
- **Beats** (captions follow): shares rise; a lone shard bounces off the lock; Bob
  proposes (real screens); a pulse runs through Z-shaped floor channels; Bob approves, his
  shard seats; you approve with one tap, yours seats; they fuse into the gold key ("it
  never exists on any phone") and push in; the door splits along the Z, gold light and a
  coin-filled interior; coins pour into your phone's sending screen; Sent!; the camera
  pulls back, a stream of identical payments crosses behind, the door closes.
- **Camera**: one continuous take along a curve through beat framings, in story order;
  damped; pointer tilt. Hero is asymmetric (headline left, world right via setViewOffset);
  portrait pulls wide shots back.
- **Stack**: Three.js 0.186 (procedural geometry, RoomEnvironment, VSM shadows), pmndrs
  postprocessing 6.39.5 (bloom on emissive seams and gold, grain, vignette), GSAP 3.15
  ScrollTrigger (one scrubbed timeline), Lenis 1.3 advanced by GSAP's ticker and feeding
  ScrollTrigger. Phone screens redraw only when their state changes.

## Decisions (2026-10-01, sixth round): the scroll story

User feedback: the pillar-like step columns were still there, and the two-phone demo was
"half-assed" because it rebuilt the UI instead of using the app's screens. Wanted: scroll
based, the page split vertically into dark and light, the proposal on a dark-mode phone,
a swirl arrow carrying the notification to a light-mode phone, the dark phone approving,
then the light phone in focus submitting; visual enough that someone who has never heard
of a multisig gets the idea without reading a flow.

- **One pinned scene** (`.story`, 640vh; `position: sticky` inside) split down the middle:
  Bob's phone on a dark half, yours on a light half. The page scroll drives everything
  through a view timeline (`view-timeline-name: --story`, `animation-range: contain`),
  so scrolling back rewinds it.
- **Real screens only**: `story_*` scenarios in `app/tool/screens/proposal_render_test.dart`
  (Bob's review step with "Propose payment", his proposal before and after approving;
  your review and your sent screen) plus Home, rendered by the app's widgets in dark (Bob)
  and light (you). The two approve screens are rendered tall and scroll inside the phone
  to the button. The only built UI is the system notification banner (it's the OS's, not
  the app's).
- **The multisig idea is carried by the vault on the seam**: three signer dots with a
  divider after two ("2 of 3"). Bob's approval fills the first, yours the second, the
  lock turns to open and a gold 12.50 TAZ coin leaves. One caption per beat, a few words
  each: "Bob proposes a payment." → "Everyone in the vault is notified." → "Bob approves.
  1 of 2." → "You approve. That's 2." → "Sent. On-chain, it looks like any payment."
- **Focus**: the phone that's acting is full size; the other dims and shrinks. Screens
  change like the app navigates (push left) and the review opens like a sheet from below.
- The three-phone stage and the step columns are gone; the story is the showcase.
- Phones stack (dark on top) under 760px, with a swirl drawn for that layout.
- Without scroll timelines (Firefox today) or with reduced motion: one screen showing the
  end (Bob approved, you sent, vault open, last caption).

## Decisions (2026-10-01, fifth round)

- The flow cards went. In their place, after headnote.in's animated product mockups (they
  animate real HTML UI, not video): **two phones playing one payment**, "Two phones. One
  payment." Bob taps New payment, the compose sheet slides up, he proposes, the sheet
  slides down; a sealed packet crosses the relay lane; your notification drops in, you
  tap it, the review sheet slides up, "Checked on this device" pops, one tap on Approve
  and sign (ripple, "Signed", the second dot fills, "Sending…", "Sent"); the packet
  returns; both phones show "Payment sent". Steps 01 Propose / 02 Notified / 03 One tap /
  04 Sent light up in sync. One 12 s CSS timeline (`--T` on `.demo-card`, keyframe
  percentages per beat); the still frame for reduced motion is your review sheet open.
  On phones the stage is scaled with `zoom`.

## Decisions (2026-10-01, fourth round)

- The pillars are now the **one-tap flow** (Notified, One tap, Sent) as live HTML cards:
  the app's real notification wording ("<vault>: payment needs your approval"), the
  Approve button with a looping tap ripple, the sent state. The point is speed and
  one-tap signing, Zafe's most distinctive feature. The onboarding illustrations went.
- The Home mockup has no backup notice (`HOME_NOTICE=0` for the render).
- **More motion**, CSS only: scroll-driven reveals where `animation-timeline` exists (side
  phones fan out, flow cards rise, the idea fills with ink, the panel grows, feature copy
  and cards meet, FAQ cascades, the vault drifts, the ghost wordmark rises); loops in the
  live cards (a vote dot fills, switches flip, "encrypted" shimmers); hover lifts. All off
  with `prefers-reduced-motion`; without scroll timelines everything is simply shown.

## Decisions (2026-10-01, third round)

- **Light only.** No dark mode anywhere on the site (`color-scheme: light`); `assets.py`
  makes light crops only, except the closing card's vault art, which is dark by design.
- **Logo intro** (CSS only, `src/styles/site.css` "Intro"): the screen is the mark's
  teal tile with the two Seam pieces locking in the middle; the tile collapses in a
  circle up into the nav logo while the pieces fly with it; the nav logo takes over,
  two rounded-square rings pulse out, the hero rises in. About 2 s; skipped with
  `prefers-reduced-motion`. The landing point is the nav logo's centre (`--land-x/y`,
  exact because the nav is a `1fr auto 1fr` grid). The mark is inlined by
  `src/components/Mark.astro` so its tile and pieces can be animated.
- **Less copy.** One line per idea; "Zcash" only in the meta description. H1 "The
  multisig nobody can see".

## Decisions (2026-10-01, second round)

The user rejected direction A below ("nah, sucks") and asked for inspiration from
vizor.cash. The page now follows Vizor's structure, studied section by section: centred
nav with the mark in the middle; centred hero (tag pill, two-line display headline, one
line, one pill button); a stage band with a notch and three members' phones showing the
same payment (home, review, sent); three illustrated pillars (Shared, Private,
Verifiable) using Zafe's own storybook illustrations; a one-sentence "idea"; an inset
dark rounded feature panel whose blocks pair copy and a two-column icon list with a live
UI card built in HTML (approval, on-device check, the real chain record, privacy
settings); a FAQ as `<details>` beside "Still got questions?"; a dark closing card over
the vault-door art; a dark footer with a ghost wordmark. Kept from round one: only
shipped claims, the real dry-run transaction, no JS, strict CSP. Not taken from Vizor:
the testimonial wall (we have none) and the scroll-driven letter reveal (needs JS).
Zafe keeps its own type (Space Grotesk, not a serif) and Verdigris colours.

## Decisions (2026-10-01, first round, superseded)

- **Direction A, "The approval"**, with C's mechanism diagram as the "How a payment moves"
  section and B only as the automatic dark theme (§5.3). H1 option 1 (§5.1).
- **What the chain sees** uses the real dry-run payment (tracker, "Testnet dry run"):
  txid `e6ccd6b707e8ee7ed8b261855272dba317495afa6659e393d3f1691e9eef0237`, block 4,422,289,
  9,166 bytes, v6, expiry 4,430,448 (read from `testnet.zec.rocks` with lightwalletd's
  `GetTransaction`); 0.01 TAZ, fee 10,000 zat, proposed by member B, approved by C and
  the phone. The members' side is typeset, not a render, because the renders use fake
  amounts.
- Copy states only shipped features; the spending-limit caveat was dropped (rules aren't
  built). The on-device check is described as `zafe-core::verify` does it (checks the
  proposed transaction against the vault's keys), not as "rebuilds".
- Zkool isn't named. The author credit links the GitHub profile.
- Icons are the app's Patina SVGs inlined in `currentColor` (magenta layer at 0.38, like
  `PatinaColorMapper`); crops and fonts come from `infra/site/assets.py`.

---

Research date: 2026-10-01, by a research agent. Screenshots of the 24 reference pages
were kept locally, not committed; the file names below (`<name>-1.png` = hero at
1280×800, `-2`/`-3` = the next two scrolls) are for reference. Items marked
**[unverified]** come from secondary sources or search snippets not checked against a
primary source.

Inputs read: `spec.md` §1–2, `docs/brand.md`, `docs/tracker.md`, `infra/site/src/pages/index.astro`, `infra/site/src/styles/site.css`, `infra/site/src/config.ts` (CSP), the live page at https://zafe-pink.vercel.app (`zafe-1..3.png`, `zafe-full.png`, `zafe-mobile.png`), and the app illustrations (`illus-sheet.png`).

---

## 1. Why pages read as "AI slop", and the current page against that list

### 1.1 What critics name

- **The averaged hero.** "A centered H1, two or three lines, often with one word in a gradient… A centered subhead directly under it… explaining what the H1 just said. Two buttons side by side: a filled primary and a ghost or outline secondary, both the same size. A backdrop that fades from one tint to another, or a faint dot-grid… A product screenshot in a tilted, perspective browser frame." The proposed fixes are an asymmetric split, one CTA with the second demoted to a text link, type and a real image instead of backdrop, and a bold crop. "Asymmetry is the biggest single lever." ([Laith Junaidy, "Every AI landing-page hero is the same"](https://uxskill.laithjunaidy.com/blog/ai-landing-page-hero-generic.html)). The same author explains why: the hero is the most-screenshotted section on the web, so the model returns "the densest point in that cloud."
- **Sixteen patterns** ([Developers Digest, "AI Design Slop: 16 patterns"](https://www.developersdigest.tech/blog/ai-design-slop-and-how-to-spot-it)): Inter everywhere; *repeated "tasteful" font combos, naming Space Grotesk*; a serif-italic accent word; lavender "vibecode purple"; permanent dark mode; low-contrast grey body; gradients; coloured glows; the centred hero; **a badge right above the H1**; **coloured left borders on cards** ("almost as reliable a sign"); **identical feature cards**; **numbered 1-2-3 step sequences**; stat banner rows; emoji nav; all-caps section labels.
- **First- and second-order defaults** ([avoid-ai-design](https://github.com/funboy322/avoid-ai-design)): first-order is purple gradients, a centred hero over three icon cards, the stock order hero → logos → features → stats → pricing → CTA, `rounded-2xl shadow-lg`, icons in rounded squares, uniform fade-up motion, count-up stats, and "Elevate / Seamless / Powerful" copy. Second-order covers the "tasteful" fixes that have since become defaults of their own: cream + terracotta, near-black with one acid-green signal, broadsheet hairlines, all-caps mono labels, **one coloured word in a headline**, **decorative 01/02/03 numbering**, fake window-chrome dots, and **emerald as the fallback accent**. Its test for every design move is "would this apply to *any* similar page?"
- **The SaaS look in general** ([Overpass Studio](https://www.overpass.studio/blog/why-saas-websites-look-the-same)): template kits, copying Stripe and Linear, speed over distinction, and "vague benefit-led headlines with low-personality copy". The fix it gives: keep navigation familiar, but order and pace the sections "to match how your best customers think, not just the default hero > features > logos > pricing template."
- **Copy** ([Julian Shapiro, Startup Handbook: Landing pages](https://www.julian.com/guide/startup/landing-pages)): the header must be descriptive enough that someone who bounced after reading it "could describe to a friend exactly what it is you do." The subheader answers "How does our product work exactly?" and "Which features make the header's claim believable?" Bad examples: "Supercharge your collaboration!" Good ones are concrete: "Groceries delivered in 1 hour."

### 1.2 Checklist of tells (used below and in §5.5)

| # | Tell | Why it reads as generated |
|---|---|---|
| T1 | Eyebrow or badge label above the H1 | Signposting that repeats the H1 |
| T2 | Slogan H1 with a comma cadence ("Do X together, in Y.") | It could belong to any product in the category |
| T3 | A subhead that restates the H1 | Adds no mechanism or proof |
| T4 | A filled button and a ghost button, same size, side by side, repeated at the bottom | The template's CTA block |
| T5 | Every section in the same container width, same padding, alternating tinted bands | No rhythm, so nothing matters more than anything else |
| T6 | Generic H2s: "How it works", "Built for…", "Why X?" | Headings that carry no claim |
| T7 | 1-2-3 numbered steps in rounded cards with circle numerals | Stock pattern |
| T8 | A 3×2 grid of identical cards (icon or accent bar, title, two lines) | Stock pattern; flattens six ideas of different weight into equals |
| T9 | Coloured left border on callouts | Named tell |
| T10 | One radius and soft shadow on everything | `rounded-2xl shadow-lg` |
| T11 | The accent colour sprinkled everywhere (labels, bars, table cells, numerals) | The colour stops meaning anything |
| T12 | A device mockup repeated at the same size, on the same side | Filler rather than evidence |
| T13 | Abstract feature nouns: "Private by default", "Non-custodial", "Secure" | Every wallet says these (Zodl: "Privacy / Self-Custody / Consent"; Vizor: "Private / Verifiable / Secure") |
| T14 | No people, no date, no version, no mechanism drawing | Nothing proves someone made it deliberately |
| T15 | A centred closing CTA that repeats the hero buttons | Template ending |

### 1.3 The current Zafe page, section by section

Screens: `zafe-1.png`, `zafe-2.png`, `zafe-3.png`, `zafe-full.png`, `zafe-mobile.png`.

The copy is better than the layout. It is specific, honest and short, and it already has the most useful content (the Safe comparison, the caveats). What makes it look generated is structure and styling: every section uses the template shape.

| Section | What's there | Tells | Notes |
|---|---|---|---|
| Top bar | Logo, two anchor links, a teal pill "Download" | T4 (the pill repeats the hero CTA) | Fine, but three calls to download before the page has explained anything. |
| Hero | Teal eyebrow "Shielded multisig for Zcash"; H1 "Move your team's ZEC together, in private."; grey lead; filled + outlined pill buttons; fine print; phone mockup of Home on the right with a soft teal shadow | T1, T2, T3, T4, T12 | The H1 works for any team wallet; "in private" is the only Zcash-specific part. The lead is the actual pitch (approvals on phones, the chain can't tell), so the hero headline hides the one thing only Zafe can say. The phone shows a balance, which every wallet shows, not the job of approving a payment together. |
| "Nothing on-chain gives it away" | Prose left, 3-row comparison table right, white band | T5, T11 (brand-teal "yes" cells) | The strongest idea on the page, but it is told rather than shown, and it sits at the same weight as every other section. Colouring Zafe's cells teal turns a fact table into a sales checklist. |
| "How it works" | Three numbered rounded cards with teal circle numerals; phone of proposal review on the right | T6, T7, T10, T12 | A second phone in the same frame, on the same side, at the same size. It lazy-loads blank in a full-page capture (`zafe-full.png`). The mechanism (FROST shares, relay, on-device check) is described in step text, never drawn. |
| "Built so nobody has to be trusted alone" | Six identical cards, 3×2, each with a 28 px teal bar above the title | T8, T10, T11, T13 | "No blind signing" is the most important security claim on the page. It gets the same box as "Encrypted backups". The teal bars are decoration. |
| "Before you use it" | Four caveats with orange left borders in a 2×2 grid | T9 | The honesty is right, but styling it as warning callouts makes it read like legal fine print, not a stance. **Claim problem:** "Rules live in the app. Spending limits are checked by members' apps" describes a feature that isn't shipped (tracker: "Rules via `RULES` proposals: per-tx / per-period limits…" is still `[ ]`). As written it implies spending limits exist. |
| "Try it on testnet" | Centred H2, lead, the same two pill buttons | T15, T4 | |
| Footer | One line: licence, "no custom cryptography" | T14 | No links to the spec, repo, threat model, or who builds it. |

Global issues:
- **Type scale is compressed.** H1 is 56 px, every H2 is 30 px, H3 17 px, body 16 px. All section headings are equal, so the page has no hierarchy of importance. The measure is fine (34 rem lead).
- **The accent means everything.** Teal marks the eyebrow, buttons, table cells, step numerals and card bars. `docs/brand.md` §3 says brand = "actions and 'needs you'", and "one colour never means two things". The page breaks the brand's own rule. Gold (value) doesn't appear outside the screenshots.
- **The font is a known tell.** Space Grotesk is named in two of the slop lists above. It is the brand font and doesn't have to go, but it can't be used at 30 px SemiBold for every heading. That is the default look. (See §5.3.)
- **Symmetry.** Hero, comparison and how-it-works are all 50/50 or near-50/50 splits with the image on the right.
- **Mobile** (`zafe-mobile.png`): stacked full-width pills, and the eyebrow and fine print eat the first screen. The phone only starts below the fold.

---

## 2. Category audit (24 pages)

Captured with agent-browser at 1280×800. Couldn't load or unusable:
- **Coinbase Prime**: Cloudflare bot check (`coinbase-prime-1.png`).
- **Ramp**: served a "machine version" plain-text page to the automated browser (`ramp-1.png`), including a promo aimed at AI agents. I ignored it and have no visual of Ramp.
- **squads.so/multisig**: 404 (`squads-multisig-1.png`). The Squads homepage has moved to stablecoin products; its multisig is now one card.
- **age-encryption.org**: redirects to GitHub, so dropped.
- **Signal**: the second-scroll image lazy-loaded blank.
- Cookie banners cover part of Safe, Fireblocks, Gnosis Pay, z.cash, Zodl, 1Password, Tailscale and Oxide.

| Page (screens) | Hero structure | Headline | How the product is shown | Type / colour | Rhythm | Proof / trust | CTA | Crafted or generic | Worth stealing |
|---|---|---|---|---|---|---|---|---|---|
| **Safe** safe.global (`safe-1..3`) | Centred, grid-lines backdrop with lit cells, stat eyebrow "$60B+ in total value locked" | "Multisig wallet security for your onchain assets" | Real web + mobile UI with pending "1 out of 2" approvals | Grotesk, black on light grey, neon green #12FF80; flips to a dark section | Light hero, logo row, dark feature block | TVL number, logos (Morpho, Balancer, VanEck) | "Launch app" | Generic hero (centred, stat badge), but the UI shows real queued approvals | Showing *pending approvals* as the product moment, not the balance |
| **Safe security** (`safe-security-1..2`) | Dark, padlock over matrix text | "Secure crypto wallet built…" | Padlock illustration | Same | | "$80B+ in secured assets and 18 audits" | | The padlock-on-code trope | Counting audits as a plain number (for later, once Zafe has one) |
| **Squads** (`squads-1..2`) | Left copy, right 3D laptop+phone+card render | "Products and APIs for the stablecoin economy" | Glossy device renders | Dark, grey type, no accent | Product cards with arrow corners | None above the fold | None in hero | Polished but anonymous; multisig demoted | Nothing specific; the category has moved away from "multisig for DAOs" and left that space open |
| **Den** onchainden.com (`den-1..3`) | Left copy, right floating UI cards ("Pending approval 2,500,000 USDC · 2 of 3 · Approve") | "Automate digital assets without getting hacked" (first word greyed) | Built UI fragments, not screenshots | System sans fallback, navy, orange | Hero, logos, 4-stat row, bento cards | Badge pills "SOC 2 Type II", "Multiple 3rd-party Security Audits"; "Three independent layers protect every transaction… On device: validated locally before being signed" | "Book a demo" + "Open the app" | Template (badges, stat row, bento) but the **approval card** is exactly the right object | Crop the product down to *one approval card* as the hero image; list security as layers that each stop a named failure |
| **Fireblocks** (`fireblocks-1`) | Left H1, right gradient slab with glass cards | "Infrastructure that powers financial possibility" | Glassmorphism fake UI | Navy, pastel gradient | Logo row | Bank logos (BNY, BNP) | | Vague headline, gradient glass: first-order slop at enterprise budget | Nothing; an example of what to avoid |
| **Gnosis Pay** (`gnosispay-1..2`) | Left condensed all-caps H1, right isometric card stack | "Ship stablecoin card programs in minutes" | 3D card render, code snippet in a fake window | Condensed display, indigo + lime | Stat row, partner logos, cream bento cards with lime borders | Volume stats, Visa/Circle/Safe logos | "Book a demo" / "View API docs" | Loud and branded, still bento + stat row | Specific promise with a unit of time |
| **Zcash** z.cash (`zcash-1..2`) | Left serif H1, right radial line drawing with one gold ray | "Zcash is encrypted electronic cash." | Abstract line art; then a full-bleed "Stay shielded." over Orchard circuit code | Big serif display, off-white #F1F0EE ground, black, gold as one accent line | Stat row (price, supply, shielded ZEC) | Live network stats | "Use Zcash" / "Learn Zcash" (square, tracked caps) | **Crafted**: a definition as headline, one gold line in a grey drawing | Colour discipline (gold appears once, as the meaningful ray); a definition-style H1 |
| **Zodl** (`zodl-1..2`) | Left small wide display H1, right real phone | "Zcash-powered mobile wallet / bringing unstoppable private money to billions" | Real phone UI (dark), "Transparent Balance Detected → Shield" | Wide extended display, violet (not amber on the site today) | Dark band with 3 centred cards "Privacy / Self-Custody / Consent" | None visible | "Download" | Generic mid-page (T8, T13) | "You don't need to know how blockchains work to use private money." A plain-language promise to non-experts |
| **Vizor** (`vizor-1..3`) | Centred serif H1 with a pill badge "Introducing Vizor" | "The Zcash Wallet You've Been Missing" | Laptop + phone renders | Serif display, crimson | 3 columns with painted knight/book/shield illustrations and hairline dividers | FAQ: "open source… **A formal third-party audit is planned.**" | "Get Vizor" | Centred hero + badge (T1), but the painted illustrations are ownable | Answering "Is it safe to use?" in an FAQ, with the audit status stated plainly |
| **Keplr** (`keplr-1`) | Centred all-caps H1 on dark grid with glow | "Your multichain gateway" | Phone + laptop | Heavy grotesk caps, cyan | | | "Get Keplr Wallet" | First-order slop (grid, glow, centred) | Nothing |
| **Signal** (`signal-1..2`) | Left H1, two tilted phones with real conversations | "Speak Freely" | Real UI with real-looking people and messages | Inter-like, periwinkle ground | Alternating text/image rows | "We can't read your messages or listen to your calls, and no one else can either." | "Get Signal" (one button) | Plain but confident; one CTA | One CTA; the security claim written as what *we* can't do |
| **Proton Pass** (`proton-1..2`) | Centred serif H1, 2 buttons, rating row | "The best free password manager" | Phone with floating cards | Serif + violet | Pricing table right after hero | "100 million users", star rating, press logos | 2 buttons | Template | Nothing new |
| **Tor Project** (`tor-1`) | Centred light H1 on purple | "Browse Privately. Explore Freely." | Hand-drawn spot illustrations below | Source Sans light, purple | Illustration + caps label rows | Non-profit | One outlined "Download Tor Browser" | Dated, but sincere | Hand-drawn spot illustrations can carry a privacy brand without padlocks |
| **Mullvad** (`mullvad-1..3`) | Top bar **"Not using Mullvad VPN · Meerut, India · Check for leaks"**; left H1; tilted app | "Privacy is for the people" | Real app (connected, server name, "Quantum resistance" chips) | Source Sans, navy, yellow, mono caps buttons | Full-bleed photo of a "Chat Control" booth, then a film | Price stated ("€5/month"), "No logging. Anonymous accounts." | Square yellow buttons | **Crafted**: opinionated, political, specific | **Live proof**: show visitors what is exposed about them right now. Zafe's version: show what the chain exposes about a Zafe payment |
| **1Password** (`1password-1..2`) | Centred H1 with Business/Personal toggle | "Secure access for every human…" | Big admin UI with a "Finance Team" vault and people list | Dark to blue | | | "Get started free" | Template | Showing *a team and its permissions* as the product (like Zafe's signers) |
| **Mercury** (`mercury-1..2`) | Centred H1 on a photo landscape; desk with laptop | "Radically different banking" | Photo, then dashboard on a laptop | Serif-ish sans, periwinkle | Photographic | **In the hero: "Mercury is a fintech company, not an FDIC-insured bank. Banking services provided through…"**, plus a footnote ¹ on "banking" | Email field + "Open account" | Mood-led; the disclaimer is the interesting part | Put the one legal/status truth *in the hero*, typeset as calm fine print, not a warning |
| **Stripe Treasury** (`stripe-1..3`) | Centred H1, 2 buttons, big dashboard over a gradient | "Manage your money and payments, together on Stripe" | Real dashboard with ledger rows | Söhne-like, navy + purple | Problem/solution two-column prose; **two-tone headings** ("Move your money faster for less." dark + grey continuation) | Customer logos | "Start now" / "Explore the docs" | Crafted details on a template skeleton | The two-tone heading (claim + explanation in one typographic block); the problem/solution columns |
| **Linear** (`linear-1..3`) | Left-aligned, very large H1; product UI bleeds off the right edge | "The product development system for teams and agents" | Real app with real-sounding conversation ("Right now we show a spinner forever…") | Inter Display, near-black, almost no accent | Logo row; features in 3 columns separated by **hairlines, no boxes**; section H2 left, paragraph right (5/7 split) | Logos | "Sign up" | Crafted; the dark style everyone copies | Product shown *mid-conversation*, with named people doing a job; hairline columns instead of cards; asymmetric H2/paragraph split |
| **Arc** (`arc-1..2`) | Centred serif H1, one big button, wavy edges, noise texture | "Meet Dia, the next evolution of Arc" | Real browser UI | Serif display, electric blue | Press quotes marquee | "**FYI: Arc receives Chromium updates only. For active security patches… download Dia instead.**" | One button | Distinct voice | Status honesty in one sentence right next to the download button |
| **Teenage Engineering** (`te-1`) | Typographic nav as a grid of pictograms; huge custom display type; hand-drawn comic | "Daily life of Mr. Update" | Illustration, product lists as a spec table | Custom display, black, one orange | Editorial | | | Entirely its own | A typographic system where numbers and specs are designed objects (version numbers set large beside the headline) |
| **Tailscale** (`tailscale-1`) | Centred H1, one button + text link | "The best secure connectivity platform for the AI era" | Icon row | Inter, grey | | | "Start connecting devices" + text link "Contact sales" | Generic homepage; see §4 for the "How Tailscale works" post | One button + a text link, not two buttons |
| **Oxide** (`oxide-1..2`) | Left H1 bottom-aligned, terminal top-left, rack photo right labelled "FIG. 1 OXIDE CLOUD COMPUTER" | "On-prem that feels like the public cloud" | Real hardware photo, real CLI | Grotesk + mono, dark, green | Two-tone paragraph heading ("One integrated platform. *Compute, storage…*") | Customer logos (LLNL) | "Try now" | Crafted; the "FIG." mono labels are becoming a second-order default | Pairing the real artefact (CLI, rack) with a plain claim |
| **Zkool** (`zkool-1`) | Docs-site hero | "The swiss-army wallet for Zcash" | None | Docs theme | 6-cell feature grid | | "Get Started" | Docs template | This is the competitor a Zcash reader will ask about (FROST multisig for 2–5 people, desktop and mobile, per [ZecHub](https://x.com/ZecHub/status/1960327834167033868)) **[unverified detail]** |

Patterns across the set:
1. **The better pages show one real object doing the job**: Den's approval card, Linear's issue thread, Mullvad's connected app, Signal's chat. The weaker ones show a balance or a glossy render.
2. **Honest status lines are a known device among respected brands** (Mercury's hero disclaimer, Arc's "FYI", Vizor's "audit is planned", Mullvad's price). None of them styles honesty as a warning box.
3. **Multisig incumbents have left the DAO-committee position.** Squads leads with stablecoin APIs, Den with enterprise, Safe with TVL. Nobody speaks to a five-person grants committee.
4. **Zcash ecosystem pages** use serif or wide display type and gold/amber or violet. Verdigris teal plus a restrained grotesk is visually open in this set.
5. **Zafe's site has no cookie banner, analytics or third-party requests.** Almost every commercial page above opens with a consent wall. That absence is a proof device for a privacy product. Say it in the footer.

---

## 3. The audience: how treasury and grant committees choose a multisig

### 3.1 What the sources say

- **Independent verification before signing is now the top concern.** In the Bybit theft (Feb 2025, ~$1.4–1.5B), attackers compromised a Safe developer machine and injected JavaScript into the Safe{Wallet} web UI, so signers approved a transaction that "looked normal but rerouted the funds". The lesson drawn: "If you approve a transaction you cannot fully read on a trusted screen, you are trusting the screen, not the math." ([Certora](https://www.certora.com/blog/bybit-hack-multisig-wallet-security), [Huntress](https://www.huntress.com/threat-library/data-breach/bybit-cryptocurrency-exchange-data-breach), [summary via odinscan](https://odinscan.ai/blog/bybit-1-5-billion-hack-explained)). Zafe's "each phone rebuilds the transaction from its own wallet state and checks it before signing" (spec §1.2 goal 4) answers this directly. It is the most relevant thing Zafe can tell a treasury person.
- **SEAL's multisig framework** ([Secure Multisig Best Practices](https://frameworks.securityalliance.org/wallet-security/secure-multisig-best-practices/), [SFC: Multisig Operations](https://frameworks.securityalliance.org/certs/sfc-multisig-ops/)) is the checklist serious signers use: independent transaction verification; a documented signer lifecycle (add, replace, remove); signer diversity across people, entities and geographies; "minimum of 3 signers with at least 50% threshold"; "avoid N-of-N… loss of a single key would result in permanent loss"; hardware wallets; dedicated communication channels; bidirectional test transactions. Zafe's page should answer these points plainly, including the ones it doesn't meet yet (no hardware-wallet members in v1, spec §1.3; member removal needs a new vault).
- **Signers go inactive over years.** WBTC DAO had to migrate an 11-of-18 multisig because signers "had become inactive and/or lost control of their keys" ([WrappedBTC/DAO PR #12](https://github.com/WrappedBTC/DAO/pull/12/files)). Loss and recovery rules are a first-screen concern, not fine print. For Zafe they are harsher than for Safe ("Lost keys below threshold: funds lost permanently", spec §1.1).
- **Coordination friction** is the everyday complaint: finding signers who are asleep or offline, 16–24 h setup, weeks to train contributors ([Namefi threat-model essay](https://namefi.io/r/en/blog/do-multisig-wallets-actually-improve-security) and similar). **[unverified]** Specific figures circulating ("GitcoinDAO 72-hour delay", "47% of r/DAO complaints") come from low-quality blogs; don't cite them.
- **The Zcash community specifically:**
  - The ZCG grants treasury is "a multisig wallet managed by ECC, ZF, and Shielded Labs" **[unverified; from a search snippet of forum minutes]**. That is exactly Zafe's customer, and they are cryptographers.
  - On a FROST-multisig grant proposal ([TSSK thread](https://forum.zcashcommunity.com/t/threshold-shielded-signing-kit-tssk-frost-powered-multisig-for-zcash/52937)), reviewers asked how it differs from ZF's existing frost-tools and whether it was redundant with Zkool. A ZF engineer said the proposal read as **"AI-generated"**. Another objected to the word: "Multisig implies multiple signatures… TSS uses secret sharing for one key." Skepticism lifted only when the team **published working code, a terminal demo and tests**.
  - Users asking about multisig get pointed to Zkool and the ZF demo, and the replies describe real confusion: which address to share, and synchronised stages across participants ([forum "Multisig" thread](https://forum.zcashcommunity.com/t/multisig/52373)).

### 3.2 What they need to see first, in order

1. **What it is, in their words and precisely**: a shared shielded vault; t-of-N approval; FROST threshold signatures, so the chain sees one ordinary signature. Use "multisig" as the familiar word and name the mechanism in the next sentence, which answers the terminology objection before anyone raises it.
2. **Proof it exists**: real app screens, a real testnet payment, the repo, the spec.
3. **How keys work and what each party can do**: members, relay, Zafe. What the relay sees (metadata, encrypted envelopes). What happens if a phone is lost.
4. **Failure modes and limits**: testnet, no audit, below-threshold loss is permanent, members keep view access forever, no hardware signers yet.
5. **Why this and not Zkool or the ZF demo**: asynchronous proposals over a relay with notifications, up to 15 members, a check on every device, mobile-first. State only verifiable facts about Zafe. Don't characterise Zkool beyond public facts.
6. **Who is behind it** and how to reach them. The current page has no name, no contact and no changelog.

### 3.3 What makes them distrust a page

- Anything that looks templated or AI-written. This community has said so in public (above).
- Claims ahead of the code: audits, "secure", "enterprise-grade", or features not shipped. Today that means the spending-limit caveat.
- Vague security nouns ("military-grade", "battle-tested") with no mechanism.
- Hiding the trade-offs, or burying them in warning callouts.
- Third-party scripts, trackers and cookie walls on a privacy product.

---

## 4. Craft methods (with sources)

- **Narrative: problem → mechanism → proof, and "show the product doing the job".** Shapiro's hero is a descriptive header, then a subheader that explains *how*, then objections handled in features ([julian.com](https://www.julian.com/guide/startup/landing-pages)). Demand Curve: show "the product in action" rather than abstract graphics, and continue the headline's story in the CTA label ([Demand Curve, above the fold](https://www.demandcurve.com/playbooks/above-the-fold)). Overpass: order sections "to match how your best customers think." For this audience that means: what it is → what the chain sees → how a payment is approved → what could go wrong → proof and source.
- **Explaining a mechanism with a progression of diagrams.** Tailscale's "How Tailscale works" builds from hub-and-spoke to mesh, one figure per idea, and uses plain metaphors: the coordination server is "a shared drop box for public keys" ([Tailscale blog](https://tailscale.com/blog/how-tailscale-works)). Zafe's relay is the same kind of object: a blind postbox for sealed envelopes.
- **Headlines for technical products.** Be descriptive, add a concrete number or unit (Gnosis Pay "in minutes", Mullvad "€5/month"), and avoid "Supercharge…" ([julian.com](https://www.julian.com/guide/startup/landing-pages)). The two-tone heading used by Stripe, Linear and Oxide (a claim in the text colour, then an explanation in secondary) packs headline and subhead into one block (`stripe-3.png`, `linear-2.png`, `oxide-2.png`). Use it once at most; it is heading toward default.
- **Type, measure and rhythm.** Body text at 45–90 characters per line ([Butterick, Practical Typography](https://practicaltypography.com/line-length.html)). Linear's UI redesign used a display cut for headings and the text cut for everything else, reducing noise and increasing hierarchy and density ([Linear, "How we redesigned the Linear UI"](https://linear.app/now/how-we-redesigned-the-linear-ui)). Stripe gets impact from space around a small CTA rather than its size ([Charli Marie on Stripe](https://pages.charlimarie.com/posts/what-we-can-learn-from-this-new-stripe-landing-page)).
- **Asymmetric grids.** The single strongest lever against the averaged hero ([Junaidy](https://uxskill.laithjunaidy.com/blog/ai-landing-page-hero-generic.html)). Linear's 5/7 H2-left/paragraph-right and its bleed-off-the-edge product crop are concrete examples (`linear-1.png`, `linear-3.png`).
- **Presenting security honestly.** Signal writes the claim as a limit on itself: "We can't read your messages… and no one else can either" (`signal.txt`). Mullvad states what it doesn't do and has its price and no-logging claim tested in public ([PCWorld](https://www.pcworld.com/article/395038/mullvad-vpn-review-2.html), [Engadget](https://www.engadget.com/cybersecurity/vpn/mullvad-vpn-review-near-total-privacy-with-a-few-sacrifices-130000056.html)). Mercury and Arc put their one status truth next to the CTA. Vizor answers "Is it safe to use?" with "A formal third-party audit is planned."
- **Motion restraint.** The slop lists flag uniform fade-ups, bounce easing and count-up stats ([avoid-ai-design](https://github.com/funboy322/avoid-ai-design)). With no JS allowed there are no scroll triggers anyway. Use none, or at most a CSS hover state, and honour `prefers-reduced-motion`.
- **Constraints for this site** (`infra/site/src/config.ts`): `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'`. That rules out:
  - inline `style=""` attributes and `<style>` blocks, including inside inline SVG. Diagrams must use presentation attributes (`fill`, `stroke`) or classes styled from `site.css`, and colour tokens must reach them through `currentColor` or classes for dark mode;
  - external fonts, embeds, analytics and video hosts.

  Screens stay as self-hosted WebP with explicit `width`/`height`. Dark mode is by `prefers-color-scheme` (already in place). **[verify]** Whether the subset DM Sans WOFF2 kept the `tnum` feature. Tabular figures matter for amounts in diagrams.

---

## 5. Design brief for the new page

### 5.1 Positioning

**Zafe is a multisig wallet for shielded Zcash: a committee's funds sit in one private vault, a payment leaves only after enough members approve it on their own phones, and the chain sees nothing but an ordinary shielded transaction.**

Headline options (each followed by the line that would sit under it):

1. **"Nothing leaves the vault until enough of you approve it. The chain sees an ordinary shielded payment."**
   Under: "Zafe is a shared Zcash wallet for grant committees, DAOs and small teams. Members' phones hold the vault together with FROST threshold signatures, and each phone checks a payment itself before it signs."
2. **"Three phones, one shielded vault, and a rule: two must approve."**
   Uses a concrete configuration as the hook (Shapiro's specificity). Under: "Set any rule from 2 of 2 up to 15 members…"
3. **"A multisig the chain can't see."**
   Shortest and boldest. It needs the mechanism line right under it ("FROST threshold signatures: your members' phones produce one ordinary spend signature together") to survive the "it's not really a multisig" objection.

My pick is 1 for the H1. It states the rule, the privacy property and the audience job in one breath. Option 3 can be the H2 of the "what the chain sees" section.

### 5.2 Narrative (ordered sections)

| # | Section | Job | Content | Visual device | Why / source |
|---|---|---|---|---|---|
| 0 | Top bar | Orient | Seam logo + "zafe"; text links: How a payment works, Limits, Source. **No** pill button here | Plain | Avoid T4; Tailscale's one-button discipline |
| 1 | Hero | Say what it is, show it working, state status | H1 (option 1); one-sentence mechanism; **one** button "Download for Android" + a text link "Read the spec and source"; status line set as calm fine print right under it: "Testnet only. Not yet externally audited. Open source (MIT/Apache-2.0)." | **Cropped real screen: a proposal waiting for your approval** (proposal review, "1 of 2 approved", "Checked on this device: Matches"), large, bleeding off the right edge or the fold. Not the balance screen | Den's approval card (`den-1.png`), Linear's bleed crop (`linear-1.png`), Mercury/Arc status line (`mercury-1.png`, `arc-2.png`) |
| 2 | What the chain sees | The unique proof | H2 "A multisig the chain can't see." Two columns of unequal width: left, what members see (the sent-payment screen: amount in gold, recipient, memo, 2 of 3 signatures); right, what the chain sees for **the same testnet transaction** (txid, block height, shielded inputs and outputs, no amounts, no addresses, one signature). Then the Safe comparison table below as a plain reference table, with no brand colour on Zafe's column | Real screen + a typeset "block explorer view" of a real testnet tx **[needs a real txid from the dry run; don't fabricate]** | Mullvad's live "what is exposed" bar (`mullvad-1.png`); z.cash's use of real protocol material (`zcash-2.png`) |
| 3 | How one payment is approved | Mechanism | One horizontal diagram, left to right: a member proposes → the relay (drawn as a postbox holding sealed envelopes; "sees who talks to whom and when, never amounts, addresses or keys") → each member's phone rebuilds the transaction from its own copy of the vault and compares → approvals → t signature shares → one ordinary signature → Zcash network. Short captions under each node, no cards | Inline SVG in Patina style (1.7 px strokes, 38% fills), teal only on the "needs your approval" node, gold only on the amount | Tailscale's diagram progression and "drop box" metaphor; brand colour roles (`docs/brand.md` §3) |
| 4 | Your phone checks before it signs | Answer the Bybit-era fear | Short paragraph: there's no web page to tamper with; each member's app rebuilds the transaction and refuses to sign if it doesn't match the proposal. One sentence of context on why this matters (signers approving a UI that lied), with a link | Close crop of "Checked on this device: Matches", annotated with 2–3 hairline callouts | Certora/Huntress Bybit write-ups; SEAL "independent transaction verification" |
| 5 | Setting up a vault | Show keys and ceremony | 2 ≤ t ≤ N ≤ 15; invite by link or QR; every phone takes part in the key ceremony, and no device ever holds the whole spend key; compare a safety number; save an encrypted backup. What each member holds, as a small table (their key share: unique; the vault's viewing secret: shared by all) | `key_shards` illustration (shares, not a whole key) as the one illustration on the page, or a 3-phone diagram; a small keys table from spec §2.2, simplified | Uses a real brand asset; SEAL signer-lifecycle questions |
| 6 | What else it does | Features, dense | A two-column definition list with hairline rules, each row a Patina icon at text size + one plain sentence: batch payments (up to 50 from a CSV) with memos; notifications; approval window per vault; Tor for every connection (fail-closed); encrypted backups; viewing key for an auditor; CSV export made on the phone; encrypted wallet database; biometric gate before signing. **Only shipped items** (tracker) | List, not cards | Linear's hairline columns (`linear-3.png`); avoids T8 |
| 7 | Limits, before you trust it with anything | Honest trade-offs as a stance | Same type weight as every other section, no orange borders. Testnet only; no external audit yet, mainnet waits for it; every member sees the vault's full history, even after leaving; removing someone fully means a new vault; if more members lose phones and backups than the vault can spare, funds are gone and nobody can recover them; no hardware-wallet signers yet; the relay sees metadata. **Drop the spending-limit caveat** until rules ship | Plain numbered prose list or a two-column "Zafe can't / Zafe won't" layout | Signal's "we can't…", Mullvad's candour, Vizor's audit line |
| 8 | Built in the open | Who and how to verify | Repo, `spec.md`, licence; libraries (zcash_client_backend, reddsa FROST, no custom cryptography); who builds it (a name and contact **[needs user input]**); a changelog or "last updated" date | Text with links; maybe a small list of the upstream crates with versions | Answers "who is behind it"; TE's specs-as-objects idea |
| 9 | Try it | Concrete next step | "You need two or three Android phones and some testnet ZEC (TAZ)." A 3-line checklist and the download button. Not centred | Left-aligned block | Replaces T15 |
| 10 | Footer | Proof by absence | "This site sets no cookies, runs no analytics and loads nothing from other servers." Licence, source, spec | Plain | Contrast with the cookie walls in §2 |

### 5.3 Visual direction

Three directions that differ in substance:

**A. "The approval" (recommended).** The page is built from the app's own objects: the proposal, the signer dots, the ledger row, the amount in gold. Light ground (#F1F5F5 window, #FFFFFF panels) with a full dark theme from the brand tokens.
- *Grid:* 12 columns, 72 rem max width, 24 px gutters. Hero copy spans columns 1–7 and the screen crop spans 7–12 and bleeds off the edge. Section headings sit in columns 1–4 with body in 5–11 (Linear's 5/7). Vary section heights and padding: the mechanism section is tall, the features list is dense and tight.
- *Type:* Space Grotesk Medium (500, not 600) for display only, at ≥40 px, tracking −0.02 em. At that size its idiosyncratic `a`, `g`, `y` and `t` read as character rather than default. DM Sans for everything else. Scale 1.333 on an 18 px body: 18 / 24 / 32 / 42 / 56 / 75. H1 about 64–72 px on desktop, 40 px on mobile, left-aligned. Section H2s at 42 px, sub-points at 24 px. Body 18 px/1.55 at 60–68 ch. Tabular figures for every amount.
- *Colour:* neutrals do almost all the work. Teal #00736C / #51DDD2 only on the one CTA, links, and the "needs your approval" state in screens and diagrams. Gold #835A00 / #F3BA3C only on ZEC amounts. Orange and rose only if a diagram shows an actual failure state. No tinted section bands. Separate sections with space and one hairline.
- *Imagery:* real renders, cropped to the relevant object rather than whole phones every time. At most one full phone frame on the page, in the hero or the chain-view section. One illustration (key_shards). The Patina icons are used inline at text size, never in tiles.
- *Risk:* depends on good crops and a well-drawn diagram. Weak screens will expose it.

**B. "Verdigris ink" (dark-first).** A dark ink ground (#080B0B / #111515) with the dark-theme renders; teal glows sparingly; gold amounts pop. It looks premium and matches how the vault card reads best (brand doc §3 notes the dial glow is best on dark).
- *Risk:* it lands in the Safe / Squads / Linear / Keplr default ("permanent dark mode" is a named tell), and dark + glowing accent + money pulls toward the trading-app feel the brief rules out. Only choose this with no glows and a strictly type-led layout.

**C. "Mechanism first" (diagram-led).** The hero is the diagram: N phones around one vault, the relay as a postbox, one ordinary signature leaving for the chain. Product screens come second. This suits the cryptographer part of the audience and is the most distinctive.
- *Risk:* an abstract hero fails Shapiro's "what do you sell" test for non-experts, and a diagram that isn't drawn with real care looks like clip art. It also needs the most drawing work.

**Recommendation: A, with C's diagram as section 3 and B as the automatic dark theme.** A shows the product doing the one thing only it does. C's diagram carries the mechanism where it belongs. B stays available through `prefers-color-scheme` without becoming the identity.

### 5.4 Things to use and not use from the brand kit

- Use: Seam logo small in the top bar, and the wordmark "zafe" in lower case (brand doc §5); Verdigris tokens as defined; Patina icon style for diagram nodes; the app's own renders; key_shards.
- Don't: the stone-wall vault illustrations (welcome_vault, join_doorway) in the hero. They lean on the vault/door/key tropes the logo work deliberately avoided, and their storybook tone fights "deliberate and calm" at large size. They are fine inside the app.

### 5.5 Do-not list (from the slop audit)

1. No eyebrow or badge above the H1.
2. No two same-size buttons side by side. One button; everything else is a text link.
3. No repeated CTA block at the bottom; end with the concrete "what you need to try it" step.
4. No 1-2-3 step cards with circled numerals. Show the payment's path as one diagram.
5. No grid of identical feature cards; use a hairline definition list.
6. No coloured left borders; the limits section is normal typography.
7. No decorative accent bars, and no teal on labels, numerals or table cells. Teal = action / "needs you", gold = money, nothing else.
8. No alternating tinted bands with identical padding; vary density and use space.
9. No second phone mockup at the same size on the same side. Crop to the object.
10. No generic headings: "How it works", "Built so…", "Why Zafe?", "Features".
11. No abstract security nouns ("Private by default", "Non-custodial", "Secure") as titles. State what can't happen and who can't do it.
12. No padlocks, shields, keyholes, glowing grids, gradient blobs, glass cards, or matrix text.
13. No stat rows or invented numbers. Zafe has no TVL, and inventing traction would sink trust with this audience.
14. No fade-up or scroll animations (and there's no JS for them anyway).
15. No claims beyond `docs/tracker.md`: no spending limits, audits, iOS, hardware wallets, or "production-ready".
16. No one coloured word in the headline, no serif-italic accent word, no all-caps mono "FIG." chrome.
17. No Space Grotesk SemiBold at 30 px for every heading. Display sizes only, with a real scale.

### 5.6 Open items for the user

- A real testnet transaction id (from the 2026-09-30 dry run) for the chain-view section. The page must not fake it.
- New renders: proposal awaiting approval (hero crop), sent payment with signatures, the "Checked on this device" state, signers list. `app/tool/screens/proposal_render_test.dart` and `home_render_test.dart` exist, but `infra/site/public/assets/screens/` has only `home_*` and `proposal_review_*` today.
- Who is behind Zafe: name, contact, and whether to link a forum thread.
- Whether to mention Zkool by name in an FAQ ("How is this different from…") or only state Zafe's facts.

### 5.7 Idea → reference map

| Idea | Source page | Screenshot |
|---|---|---|
| Approval card as the hero object | https://onchainden.com | `den-1.png` |
| Product crop bleeding off the edge; hairline columns; 5/7 heading split | https://linear.app | `linear-1.png`, `linear-3.png` |
| Status truth in the hero as fine print | https://mercury.com | `mercury-1.png` |
| One-line honest status next to download | https://arc.net | `arc-2.png` |
| Audit status stated plainly in an FAQ | https://vizor.cash | `vizor.txt`, `vizor-3.png` |
| "What is exposed right now" live proof → "what the chain sees" | https://mullvad.net/en | `mullvad-1.png` |
| Security claim as a limit on yourself | https://signal.org | `signal-2.png`, `signal.txt` |
| One gold accent with meaning; definition-style H1; protocol code as texture | https://z.cash | `zcash-1.png`, `zcash-2.png` |
| Pending approvals as the product moment | https://safe.global | `safe-2.png` |
| Two-tone heading (claim + explanation) | https://stripe.com/treasury, https://oxide.computer | `stripe-3.png`, `oxide-2.png` |
| Specs and version numbers as designed objects | https://teenage.engineering | `te-1.png` |
| Hand-drawn spot art can carry a privacy brand | https://www.torproject.org | `tor-1.png` |
| One button + text link | https://tailscale.com | `tailscale-1.png` |
| Mechanism by diagram progression; "drop box" metaphor | https://tailscale.com/blog/how-tailscale-works | (article, not captured) |
| What to avoid: gradient glass, vague H1 | https://www.fireblocks.com, https://www.keplr.app | `fireblocks-1.png`, `keplr-1.png` |
| What to avoid: padlock on matrix text | https://safe.global/security | `safe-security-1.png` |
| Category gap: incumbents moved to enterprise/stablecoins | https://squads.so | `squads-1.png`, `squads-2.png` |
| Competitor a Zcash reader will ask about | https://hhanh00.github.io/zkool2/ | `zkool-1.png` |

## Sources

- Laith Junaidy, [Every AI landing-page hero is the same](https://uxskill.laithjunaidy.com/blog/ai-landing-page-hero-generic.html)
- Developers Digest, [AI Design Slop: 16 Patterns](https://www.developersdigest.tech/blog/ai-design-slop-and-how-to-spot-it)
- [avoid-ai-design](https://github.com/funboy322/avoid-ai-design)
- Overpass Studio, [Why SaaS websites look the same](https://www.overpass.studio/blog/why-saas-websites-look-the-same)
- Julian Shapiro, [Landing page copywriting](https://www.julian.com/guide/startup/landing-pages); Demand Curve, [Above the fold](https://www.demandcurve.com/playbooks/above-the-fold)
- Charli Marie, [What we can learn from this new Stripe landing page](https://pages.charlimarie.com/posts/what-we-can-learn-from-this-new-stripe-landing-page)
- Matthew Butterick, [Line length](https://practicaltypography.com/line-length.html)
- Linear, [How we redesigned the Linear UI](https://linear.app/now/how-we-redesigned-the-linear-ui)
- Tailscale, [How Tailscale works](https://tailscale.com/blog/how-tailscale-works)
- SEAL, [Secure Multisig Best Practices](https://frameworks.securityalliance.org/wallet-security/secure-multisig-best-practices/), [SFC: Multisig Operations](https://frameworks.securityalliance.org/certs/sfc-multisig-ops/)
- Bybit: [Certora](https://www.certora.com/blog/bybit-hack-multisig-wallet-security), [Huntress](https://www.huntress.com/threat-library/data-breach/bybit-cryptocurrency-exchange-data-breach), [odinscan](https://odinscan.ai/blog/bybit-1-5-billion-hack-explained)
- [WrappedBTC DAO multisig migration](https://github.com/WrappedBTC/DAO/pull/12/files)
- Zcash forum: [TSSK FROST multisig proposal](https://forum.zcashcommunity.com/t/threshold-shielded-signing-kit-tssk-frost-powered-multisig-for-zcash/52937), [Multisig](https://forum.zcashcommunity.com/t/multisig/52373), [Community Grants category](https://forum.zcashcommunity.com/c/grants/33); [Zkool docs](https://hhanh00.github.io/zkool2/), [ZecHub on Zkool](https://x.com/ZecHub/status/1960327834167033868)
- Mullvad reviews: [PCWorld](https://www.pcworld.com/article/395038/mullvad-vpn-review-2.html), [Engadget](https://www.engadget.com/cybersecurity/vpn/mullvad-vpn-review-near-total-privacy-with-a-few-sacrifices-130000056.html)

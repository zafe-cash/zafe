---
name: landing-page
description: Design, build and ship the product's landing page (or redesign it) the way a product designer plus a 3D/motion designer would - research, a show-don't-tell scroll story in one WebGL world with the app's real screens, intro logo motion, custom cursor and text motion, under a strict CSP - then verify it frame by frame and deploy. Use when asked for a landing page, website, marketing site, site redesign, hero/scroll animation, "make the site look great", or a page section (features, CTA, FAQ) in the 3D style.
---

# Landing page

How Zafe's site (`infra/site/`, Astro) went from a plain page to "the Seam Vault", a 3D
world behind a scroll story (docs/site.md has every decision round). Read `docs/site.md`,
`docs/brand.md` and the website notes in `docs/app-notes.md` first; the user's standing taste is
in memory (Vizor-like layout, light only, few words, logo motion).

**Where motion goes** (the user's call, round eleven): the home page `/` stays calm, like
safe.global (no JS, CSS reveals, honest security and testnet status, no invented stats
or logos); the 3D story lives on `/showcase`, linked from the nav and "Watch it in 3D".
**The two pages must never look alike** (round twelve, "showcase is the replica of the
main site"): home pictures are the app's real screens in phone frames (`.lp-phone`), with
one still of the world at most (the showcase teaser), never the same backdrop section
after section; the showcase has its own entrance (no logo intro, a title card, a light
fixed nav with the way home, a slim footer). Sections 3-4 below describe the showcase;
build new home sections from the `lp-*` patterns in `index.astro`. Test nav clicks
(logo, Home, Showcase) on both pages, desktop and phone, before shipping.

## 1. Brief: product, audience, the one idea

- Who lands here (Zafe: DAOs and teams who hold money together) and the single thing they
  must get without reading: *nobody can move the money alone, and nobody outside sees it*.
- Copy: less is more. One short line per idea, never repeat the ecosystem name ("no need
  to say zcash zcash everywhere"), only claim shipped features, real numbers only (the
  chain card is a real testnet tx).
- Layout reference the user picked: vizor.cash (centred nav with the mark in the middle,
  big headline, generous paper). Light mode only.

## 2. Research (delegate, keep building)

The user asked for research, and to "think like a 3D designer". Spawn general-purpose
agents in parallel, each writing a report to the job tmp dir, while you keep working:
1. **Choreography**: 10-20 reference sites (scroll stories, product dioramas), how 3D web
   designers plan beats, camera, pacing, cursor and text motion. Ask for ideas, not code.
2. **Tech**: Three.js + postprocessing + GSAP ScrollTrigger + Lenis under our CSP (probe
   it: no inline script/style, `connect-src 'self'`), perf on phones, fallbacks.
3. **Assets**: procedural geometry vs models, SVG extrusion of the logo, screen textures.
Fold the findings into a plan with the beats before building.

## 3. Story: show the mechanism, don't explain it

- Find a physical metaphor built from the brand: Zafe's vault door **is** the Seam mark
  (two extruded leaves), the key **is** the Z, split into shards held by the phones; two
  shards fuse (the key never exists on one phone), the door opens, coins pour into the
  sending screen. Someone who never heard of multisig should vaguely get it.
- Write beats (one caption each, ~8-10 words) and a camera framing per beat **in story
  order** (a curve through non-adjacent framings flies through the scene).
- Use the **app's real screens**: Flutter render tests (`app/tool/screens/*_render_test.dart`)
  → `infra/site/assets.py` crops → WebP → canvas textures redrawn only when their state
  changes. Never rebuild app UI in HTML (rejected as "half-assed"). Show off the best
  screen (the sending screen).
- Things the user rejected, don't bring back: generic "AI slop" sections, pillar/step
  columns, phones side by side and static, small phones, a lock instead of a vault, a
  dark/light vertical split, a cursor companion beside the system arrow, a text label
  following the cursor.

## 4. Build

Stack (all npm, bundled same-origin): Three.js (RoundedBoxGeometry, RoomEnvironment,
SVGLoader `path.toShapes(true)`, VSMShadowMap), pmndrs `postprocessing` (bloom on
emissive/gold only, grain, vignette), GSAP ScrollTrigger, Lenis.
- **CSP**: `script-src 'self'; style-src 'self'`; `build.sh` fails on any inline
  script/style/handler. JS may set `el.style` (CSSOM is allowed); markup may not.
- **One loop**: `new Lenis({ autoRaf: false })`, `lenis.on('scroll', ScrollTrigger.update)`,
  `gsap.ticker.add((t) => lenis.raf(t * 1000))`, `lagSmoothing(0)`.
- **State object** `S` (camera index, frame offsets, per-beat 0..1 values) driven by
  scrubbed timelines; the render loop reads `S` and damps the camera. **One scrubbed
  timeline per scroll range that writes a property**: two triggers writing `S.cam`
  raced after a fast scroll and left the camera short (use `endTrigger` to span
  sections). DOM captions follow `data-beat` set from the timeline.
- **One fixed canvas** behind transparent sections; opaque sections (FAQ, footer) pause
  rendering only when they cover the whole screen.
- **Composition**: asymmetric hero via `camera.setViewOffset` (headline left, world
  right). Portrait: pull wide framings back, convert the side offset into a vertical
  one, and lift the world above bottom cards (`portraitY`). Fog relative to the camera's
  distance, or far framings (the view from above) fog out.
- **Intro**: CSS-only keyframes (full-screen tile → ripple → the nav mark). Centre on
  `50%` of a fixed layer, never `50vw` (includes a classic scrollbar: ripples were off
  the logo).
- **Cursor**: replace the system pointer (the user wants it gone, not accompanied): dot
  exactly on the pointer + trailing ring; links swell the ring; the CTA pill gets a
  wrapping ring and a magnetic lean. Fine pointers and no reduced motion only; add
  `cursor: none` only after the first draw. No labels on the cursor.
- **Text**: masked word rise (own splitter, no plugin), a scramble "unshield" at most
  twice, a rolling counter for the threshold ("2 of 2 approvals").
- **Fallbacks**: no GPU, reduced motion or no JavaScript (Tor Browser "Safest": a real
  share of a privacy audience) → the same layout over **stills rendered from the world**
  (`infra/site/stills.sh`, capture mode `/?still`; also renders the social image). Gate
  them with a blocking `boot.js` (`html.js`) so the world's visitors never fetch them;
  fallback-only rules go under `:is(html:not(.js), html.stills)`. Re-render the stills
  whenever the world or the story changes. The world must never be required to read
  the page.
- **SEO**: canonical/OG/JSON-LD come from `Base.astro` props and `site` in
  `astro.config.mjs`; keep every heading level meaningful (crawlers see only the DOM).
- **Details the user notices**: every home section carries the same small `.tag` label
  above its heading (one missing one reads as a slip); compound words in narrow cells or
  labels get `<span class="nowrap">` so phones don't split them (not U+2011: the font
  subset lacks it, so it would render in a fallback font).

## 5. Verify every change by looking

```bash
(cd infra/site && ./build.sh)                         # includes the inline-code guard
python3 -m http.server 8768 --bind 127.0.0.1 --directory infra/site/dist   # background
S=.claude/skills/landing-page
# The 3D story is on /showcase; the plain server needs the .html (Cloudflare serves clean paths).
bash $S/frames.sh "http://127.0.0.1:8768/showcase.html?world=always&pointer=fine" .story 1440 900 12 "$OUT" story
python3 $S/sheet.py "$OUT" story 3 0.36               # one contact sheet, then Read it
bash $S/frames.sh "http://127.0.0.1:8768/showcase.html?world=always" .story 390 844 12 "$OUT" story_m && python3 $S/sheet.py "$OUT" story_m 6 0.5
```
- Desktop 1440x900 **and** phone 390x844, every section you touched; open the sheet and
  the key single frames; fix, rebuild, recapture before reporting.
- **On phones, check the wide 3D shots fill the width** without cropping a phone: the
  portrait pull-back for wide framings is ×1.42 (`buildCamera` in world.js: ×1.55 left the
  diorama at half the screen, ×1.3 cut the edge phones). Re-check after moving the camera
  or the phones.
- Headless Chrome renders WebGL in software (SwiftShader), so the page skips the world
  there: add `?world=always`. The world then runs at the lowest quality (softer) and
  ~1 fps, fine for screenshots, meaningless for frame rates; the bare URL checks the
  stills fallback. No fine pointer either: `?pointer=fine`; move the mouse with
  `agent-browser mouse move` twice before a cursor shot.
- **Performance** (see `docs/site.md` ninth round): keep adaptive quality, the software
  renderer skip and the give-up path working (`canvas.world` `data-quality`: 0-5 or
  `off`); hashed bundles are cached for a year via `public/_headers`.
  Measure page weight and rAF rate with a small agent-browser script in a file.
- The sandbox refuses complex inline shell and `eval` strings: put scripts in files and
  run them with `bash`.

## Improving in rounds

When asked to keep improving the site: each round, screenshot every section of both pages
(desktop and phone), write a ranked, harsh critique against the user's taste (memory
`website-style`: loves / rejected / cares about), fix the top issues, verify, ship to main,
and log the round in `docs/site.md`. **Minor issues count**: the user asks for them anyway
("fix the minor issues too"), so fix them in the round that finds them rather than
listing them as left over. Stop when a critique finds nothing at all, or after the round
limit you were given, and say what you'd do next.

## 6. Record and ship

- Add a decision round at the top of `docs/site.md` (the feedback quoted, what changed,
  what's next) and update the website notes in `docs/app-notes.md` when you learn a gotcha.
- Work in a worktree; commit and push the branch (a PR's `site.yml` run uploads a Cloudflare
  preview). **When done, push to main and clean up, always** (the user's standing rule):
  merge/push to main once the site build is OK (no need to wait for Rust/Flutter CI),
  confirm the branch is in `origin/main`, delete the remote branch, remove the worktree
  (ExitWorktree remove), `git pull --ff-only` the main checkout. Main deploys
  production: watch the Site run, then check the live URL (status 200, the CSP header,
  new sections present, a screenshot, no page errors).

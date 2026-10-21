# Zafe site: SEO audit

Audit of `infra/site` (live at https://zafe-pink.vercel.app, built `dist/`) on 2026-10-01,
against Google Search Central and schema.org. Severity: **high** = hurts indexing or how
the result looks, **medium** = missed signal, **low** = polish.

## Where things live

- Public origin: `site` in `infra/site/astro.config.mjs`. Canonical, `og:url`,
  `og:image`, `sitemap.xml` and `robots.txt` all derive from it. **Change it when the
  site gets its own domain** (and redeploy: the old URL then canonicalises to the new).
- Head tags: `src/layouts/Base.astro` (props `title`, `description`, `socialTitle`,
  `jsonLd`, `noindex`). Social image settings: `src/config.ts` (`socialImage`).
- `src/pages/sitemap.xml.ts`, `src/pages/robots.txt.ts`: build-time endpoints, no deps.
- Icons: `infra/site/icons.sh` writes `favicon.ico`, `apple-touch-icon.png` and
  `assets/icons/*` from the brand sources; `public/site.webmanifest`.

## Findings

| # | Issue | Severity | Status |
|---|-------|----------|--------|
| 1 | No `robots.txt` (404) | medium | Fixed: generated, `Allow: /` + `Sitemap:` |
| 2 | No `sitemap.xml` (404) | medium | Fixed: generated, only `/` |
| 3 | No canonical URL; `/index.html` also answers 200 (duplicate) | high | Fixed: `<link rel="canonical">` from `site` on indexable pages |
| 4 | No Open Graph / Twitter tags: shared links (chat, X, Telegram) showed no card | high | Fixed: og:type/site_name/locale/url/title/description/image (+ type, width, height, alt), `summary_large_image`. Image `/assets/og.png` is produced separately |
| 5 | `<title>` was the tagline only ("the multisig nobody can see"): no words people search for | medium | Fixed: "Zafe: private multisig wallet for Zcash teams"; the tagline stays the H1 and the share-card title (`socialTitle`) |
| 6 | Meta description fine but thin on terms | low | Rewritten (135 chars): wallet, Zcash, teams and DAOs, approval, on-chain privacy |
| 7 | No structured data | medium | Fixed: JSON-LD `@graph` on `/`: Organization (logo, GitHub `sameAs`), WebSite (site name), MobileApplication (Android, FinanceApplication, free), FAQPage (generated from the same `faq` array the page renders, so they can't drift) |
| 8 | `/favicon.ico` 404; SVG-only favicon (Google's favicon crawler and older clients want a raster) | medium | Fixed: 16/32/48 `.ico`, 180 px apple-touch-icon, 192/512 + maskable manifest icons |
| 9 | No web manifest | low | Fixed: `site.webmanifest`; the CSP's `default-src 'none'` would block it, so `manifest-src 'self'` was added (`_headers` + `config.ts`) |
| 10 | `/join` indexability | ok | Already `noindex`. Kept out of the sitemap and **not** disallowed in robots.txt (Google must crawl it to see `noindex`). No canonical on it. Gained a description and OG tags for chat previews (they fetch the URL without the `#invite`) |
| 11 | `/join/` and `/join.html` | ok | 308 to `/join` |
| 12 | `http://` | ok | 308 to https, HSTS preload |
| 13 | `lang`, viewport, charset, one H1 | ok | Present |
| 14 | Story section (the 3D payment) has no heading; crawlers see ten caption paragraphs under the hero's H1 | medium | Fixed in the redesign: `/showcase` has an H2 "How it works" |
| 15 | LCP: on a GPU the hero text appears only after the ~1.8 s CSS intro (`.home .hero > *` starts at opacity 0); without a GPU the LCP is the lazy story still (`loading="lazy"` on an image in the first viewport). Lighthouse mobile: LCP 3.2 s, FCP 1.8 s, performance 91 | medium | Home fixed 2026-10-01: after the redesign the hero still faded in from opacity 0, so Lighthouse found no LCP at all (`NO_LCP`); the H1 is no longer faded (the closing veil reveals it), desktop performance 99, LCP 2.0 s. /showcase CLS 0.384 (the title card changed size when story.js chose world or stills) fixed to 0. **Open**: /showcase has no LCP (its title card fades in over the world, tuned in round 12a); its lazy hero still is on purpose, so world visitors never fetch it |
| 16 | Contrast: `.chain-mini dt` (#7e8987 on #fdfefe, 3.57:1) | low | Fixed in the redesign (`--text-secondary` #55615F); Lighthouse accessibility 100 on both pages |
| 17 | Live site still sends `max-age=0` for hashed bundles | low | Fixed in `537851b` (`_headers`); live since (checked 2026-10-21: `immutable`) |
| 18 | No HTML 404 page (Vercel answers a plain-text 404, status correct) | low | Fixed 2026-10-01: `404.astro` (noindex), Vercel `handle: error` route in `vercel-output.sh` |
| 19 | Heading text "Questions?" | low | Fine as is |
| 20 | Link text | ok | "Get Zafe", "Source", "Spec", "Support": descriptive enough; all external links `rel="noopener noreferrer"` |
| 21 | No third-party requests | ok | Unchanged: no analytics, fonts and images same-origin |

## Notes from the guidelines

- FAQ rich results are gone for almost everyone (Google limited them to government and
  health sites in 2023 and removed the feature from Search in May 2026). FAQPage markup
  is still valid schema.org and other engines and answer tools read it; expect no FAQ
  snippet in Google.
- SoftwareApplication/MobileApplication rich results need `aggregateRating` or `review`.
  We have neither and must not invent them, so no app rich result yet; the markup still
  describes the app. Add the Play Store URL to `sameAs` / `downloadUrl` when there is one.
- Site name (WebSite `name`) is read only on the domain root (`zafe.cash`, live).
- `noindex` + robots.txt `Disallow` don't mix: a disallowed page can't be fetched, so its
  `noindex` is never seen and the bare URL can still be indexed from links.

## Build guard

`build.sh` refuses inline `<script>`, `<style>` and `style=`/`on*=` attributes. It now
allows exactly `<script type="application/ld+json">` (no other attribute): browsers never
execute that type, so `script-src 'self'` isn't involved. `Base.astro` escapes `<` in the
data so it can't close the element, and `build.sh` checks every such block parses as JSON.

## Verified

- `./build.sh` passes (guard + JSON-LD check); the guard still catches a plain inline
  script, a `type="module"` inline script and an ld+json script with an extra attribute.
- Lighthouse (local, headless, mobile): `/` SEO 100, best practices 100, accessibility 96
  (#16), performance 91 (#15). `/join` SEO 66 only because of the intended `noindex`.

## Open questions

- Custom domain: `zafe.cash` (bought 2026-10-01, `site` changed), registered in Google Search
  Console and Bing Webmaster Tools by DNS TXT record; each production deploy submits the
  sitemap's URLs to IndexNow (`indexnow.sh`). Keep `zafe-pink.vercel.app` redirecting to it.
- Play Store listing: its URL goes into `downloadUrl` and `sameAs`.

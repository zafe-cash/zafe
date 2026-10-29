# Zafe site: SEO

How `infra/site` handles search and link previews, and the audit behind it (2026-10-01,
against Google Search Central and schema.org).

## Where things live

- Public origin: `site` in `infra/site/astro.config.mjs` (`https://zafe.cash`). Canonical
  URLs, `og:url`, `og:image`, `sitemap.xml` and `robots.txt` all derive from it.
- Head tags: `src/layouts/Base.astro` (props `title`, `description`, `socialTitle`,
  `jsonLd`, `noindex`). Indexable pages get a canonical link; `noindex` pages get none.
- Share image: `socialImage` in `src/config.ts`, `/assets/og-mainnet.png` (1200×630),
  rendered by `stills.sh`.
- JSON-LD: only on `/` (`src/pages/index.astro`): Organization (`sameAs`: GitHub, X,
  Google Play), WebSite, MobileApplication (Android, FinanceApplication, free,
  `installUrl` Google Play, `downloadUrl` the newest mainnet release), FAQPage built from
  the same `faq` array the page renders, so they can't drift.
- `src/pages/sitemap.xml.ts`: `/`, `/download`, `/support`, `/showcase`, `/privacy`,
  `/docs` and every docs page from `src/docs/pages.ts`. `robots.txt` allows everything and
  names the sitemap.
- Not indexed (`noindex`, kept out of the sitemap, **not** disallowed in robots.txt so
  crawlers can see the tag): `/join`, `/waitlist`, `/waitlist-error`, `/relay` (a
  redirect), the 404.
- Icons: `infra/site/icons.sh` writes `favicon.ico`, `apple-touch-icon.png` and
  `assets/icons/*`; `public/site.webmanifest`.
- Search engines: `zafe.cash` is verified in Google Search Console and Bing Webmaster
  Tools by DNS TXT record. Each production deploy submits the sitemap's URLs to IndexNow
  (`indexnow.sh`). Pages name `zafe.cash` as canonical, so the workers.dev preview URLs don't compete with it.

## Build guard

`build.sh` refuses inline `<script>`, `<style>` and `style=`/`on*=` attributes, with one
exception: `<script type="application/ld+json">` with no other attribute. Browsers never
execute that type, so `script-src 'self'` doesn't apply. `Base.astro` escapes `<` in the
data so it can't close the element, and `build.sh` checks that every such block parses as
JSON.

## Open

- `/showcase` reports no LCP: its title card fades in over the world (tuned in round 12a,
  `docs/site.md`). Its lazy hero still is on purpose, so visitors who get the world never
  fetch it.

## Audit, 2026-10-01

Run against `https://zafe-pink.vercel.app` and the built `dist/`. Severity: **high** hurts
indexing or how the result looks, **medium** is a missed signal, **low** is polish.
Status as of 2026-10-27.

| # | Issue | Severity | Status |
|---|-------|----------|--------|
| 1 | No `robots.txt` (404) | medium | Fixed: generated, `Allow: /` + `Sitemap:` |
| 2 | No `sitemap.xml` (404) | medium | Fixed: generated (pages listed above) |
| 3 | No canonical URL; `/index.html` also answered 200 | high | Fixed: canonical from `site` on indexable pages |
| 4 | No Open Graph / Twitter tags: shared links showed no card | high | Fixed: og type, site name, locale, url, title, description, image (type, size, alt); `summary_large_image` |
| 5 | `<title>` was the tagline only, with no search terms | medium | Fixed: "Zafe: private multisig wallet for Zcash"; the tagline stays the share title (`socialTitle`) |
| 6 | Thin meta description | low | Rewritten: wallet, Zcash, approval, on-chain privacy |
| 7 | No structured data | medium | Fixed: JSON-LD (above) |
| 8 | `/favicon.ico` 404; SVG-only favicon | medium | Fixed: 16/32/48 `.ico`, 180 px apple-touch-icon, 192/512 and maskable manifest icons |
| 9 | No web manifest | low | Fixed: `site.webmanifest`; `manifest-src 'self'` added to the CSP |
| 10 | `/join` indexability | ok | `noindex`, out of the sitemap, crawlable; has a description and OG tags for chat previews (they fetch the URL without the fragment) |
| 11 | `/join/` and `/join.html` | ok | 308 to `/join` |
| 12 | `http://` | ok | 308 to https, HSTS preload |
| 13 | `lang`, viewport, charset, one H1 | ok | Present |
| 14 | The 3D story had no heading | medium | Fixed: `/showcase` has an H2 "How it works" |
| 15 | Slow or missing LCP (hero behind a ~1.8 s CSS intro) | medium | Home fixed 2026-10-01: the H1 is no longer faded in; Lighthouse desktop performance 99, LCP 2.0 s. `/showcase` CLS 0.384 → 0. Open: `/showcase` LCP (above) |
| 16 | Low contrast on chain-card labels (3.57:1) | low | Fixed in the redesign; Lighthouse accessibility 100 |
| 17 | Hashed bundles sent with `max-age=0` | low | Fixed in `537851b` (`_headers`); live `immutable` (checked 2026-10-21) |
| 18 | No HTML 404 page | low | Fixed: `404.astro` (noindex), Vercel `handle: error` route |
| 19 | Heading text "Questions?" | low | Fine as is |
| 20 | Link text | ok | Descriptive; external links `rel="noopener noreferrer"` |
| 21 | No third-party requests | ok | No analytics; fonts and images same-origin |

## Notes from the guidelines

- Google limited FAQ rich results to government and health sites in 2023 and removed them
  from Search in May 2026. FAQPage markup is still valid schema.org, and other engines and
  answer tools read it; expect no FAQ snippet in Google.
- MobileApplication rich results need `aggregateRating` or `review`. We have neither and
  must not invent them, so there is no app rich result; the markup still describes the app.
- The site name (WebSite `name`) is read only on the domain root.
- `noindex` and a robots.txt `Disallow` don't mix: a disallowed page is never fetched, so
  its `noindex` is never seen and the bare URL can still be indexed from links.

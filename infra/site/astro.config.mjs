// Static site: no client framework; JS only in hand-written files under /assets. Nothing
// may be inlined (the CSP allows only same-origin files), see README.md.
import { defineConfig } from 'astro/config';

export default defineConfig({
  // The site's public origin: canonical URLs, og:url/og:image, sitemap.xml and robots.txt
  // all derive from it (Astro.site).
  site: 'https://zafe.cash',
  output: 'static',
  build: {
    format: 'file', // /join -> join.html, as the hosts' configs in README.md expect
    inlineStylesheets: 'never',
    assets: 'assets/_astro',
  },
  compressHTML: true,
  devToolbar: { enabled: false },
  prefetch: false,
  vite: {
    build: { assetsInlineLimit: 0 },
  },
});

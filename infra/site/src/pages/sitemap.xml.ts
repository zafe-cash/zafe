// /sitemap.xml, built from `site` in astro.config.mjs. Only indexable pages: /join is
// noindex (invite page) and stays out. Add a page here when one is added to the site.
import type { APIRoute } from 'astro';

const pages = ['/', '/download', '/support', '/showcase', '/privacy'];

export const GET: APIRoute = ({ site }) => {
  if (!site) throw new Error('set `site` in astro.config.mjs');
  const urls = pages.map((p) => `  <url><loc>${new URL(p, site).href}</loc></url>`).join('\n');
  const body =
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
    `${urls}\n` +
    '</urlset>\n';
  return new Response(body, { headers: { 'Content-Type': 'application/xml; charset=utf-8' } });
};

import type { APIRoute } from 'astro';

const YEAR_MS = 365 * 24 * 60 * 60 * 1000;

export const GET: APIRoute = ({ site }) => {
  if (!site) throw new Error('set `site` in astro.config.mjs');
  const expires = new Date(Date.now() + YEAR_MS).toISOString().replace(/\.\d{3}Z$/, 'Z');
  const body = [
    `Contact: mailto:security@${site.hostname}`,
    `Expires: ${expires}`,
    'Preferred-Languages: en',
    `Canonical: ${new URL('/.well-known/security.txt', site).href}`,
    '',
  ].join('\n');
  return new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
};

// POST /api/waitlist: the iPhone waitlist (src/components/Waitlist.astro). A plain HTML form,
// so it answers with a redirect: /waitlist when the address is stored, /waitlist-error
// otherwise. Each address is one private blob named by its hash, so joining twice stores it
// once. vercel-output.sh bundles this file into a Vercel function (Build Output API, Node).
//
// Storage: a private Vercel Blob store connected to the project. The function authenticates
// with the request's OIDC token when the store id is in BLOB_STORE_ID, otherwise with
// BLOB_READ_WRITE_TOKEN (what `vercel blob create-store` sets). @vercel/blob 2.8.1 throws if
// an OIDC token is passed without a store id instead of falling back to the token
// (dist/chunk-GPSPCQKX.js:192-195), so pass it only with BLOB_STORE_ID.
// Nothing else is kept: no IP address, no user agent.
import { createHash } from 'node:crypto';
import { put } from '@vercel/blob';

const MAX_BODY = 4096;
// Deliberately loose (browsers already check type="email"); refuses only what can't be an address.
const EMAIL = /^[^\s@<>"]{1,64}@[^\s@<>"]+\.[^\s@<>".]{2,}$/;

function redirect(res, path) {
  res.statusCode = 303;
  res.setHeader('Location', path);
  res.setHeader('Cache-Control', 'no-store');
  res.end();
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks = [];
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY) {
        reject(new Error('body too large'));
        req.destroy();
      } else chunks.push(chunk);
    });
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

// Only our own pages may post here (the form is same-origin; the CSP allows form-action 'self').
function sameSite(req) {
  const origin = req.headers.origin;
  const host = req.headers['x-forwarded-host'] || req.headers.host;
  if (!origin || !host) return false;
  try {
    return new URL(origin).host === host;
  } catch {
    return false;
  }
}

export default async function handler(req, res) {
  if (req.method !== 'POST') {
    res.statusCode = 405;
    res.setHeader('Allow', 'POST');
    res.end();
    return;
  }
  if (!sameSite(req)) return redirect(res, '/waitlist-error');

  let form;
  try {
    form = new URLSearchParams(await readBody(req));
  } catch {
    return redirect(res, '/waitlist-error');
  }
  // The hidden field is empty for people; bots fill it in. Pretend it worked.
  if (form.get('website')) return redirect(res, '/waitlist');

  const email = (form.get('email') || '').trim().toLowerCase();
  if (email.length > 254 || !EMAIL.test(email)) return redirect(res, '/waitlist-error');

  const id = createHash('sha256').update(email).digest('hex');
  const oidcToken = process.env.BLOB_STORE_ID ? req.headers['x-vercel-oidc-token'] : undefined;
  try {
    await put(`waitlist/${id}.json`, JSON.stringify({ email, joined: new Date().toISOString() }), {
      access: 'private',
      contentType: 'application/json',
      addRandomSuffix: false,
      allowOverwrite: true,
      ...(oidcToken ? { oidcToken } : {}),
    });
  } catch (err) {
    // Never log the address itself.
    console.error('waitlist: store failed:', err && err.name);
    return redirect(res, '/waitlist-error');
  }
  return redirect(res, '/waitlist');
}

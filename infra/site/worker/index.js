// The site's Worker (wrangler.jsonc). Cloudflare serves dist/ on its own, without running
// this code and without counting requests; the Worker runs only for the paths in
// assets.run_worker_first:
//   POST /api/waitlist     the iPhone waitlist (src/components/Waitlist.astro)
//   /join on TESTNET_HOST  the testnet invite page (join-testnet.html)
// Anything else that reaches it is served from the assets as usual.

const MAX_BODY = 4096;
// Deliberately loose (browsers already check type="email"); refuses only what can't be an address.
const EMAIL = /^[^\s@<>"]{1,64}@[^\s@<>"]+\.[^\s@<>".]{2,}$/;

function redirect(path) {
  return new Response(null, { status: 303, headers: { Location: path, 'Cache-Control': 'no-store' } });
}

async function readBody(request) {
  if (!request.body) return '';
  const reader = request.body.getReader();
  const chunks = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY) {
      await reader.cancel();
      throw new Error('body too large');
    }
    chunks.push(value);
  }
  const body = new Uint8Array(size);
  let at = 0;
  for (const chunk of chunks) {
    body.set(chunk, at);
    at += chunk.byteLength;
  }
  return new TextDecoder().decode(body);
}

// Only our own pages may post here (the form is same-origin; the CSP allows form-action 'self').
function sameSite(request) {
  const origin = request.headers.get('Origin');
  if (!origin) return false;
  try {
    return new URL(origin).host === new URL(request.url).host;
  } catch {
    return false;
  }
}

// Each address is one KV key, the SHA-256 of the address, so joining twice stores it once.
// Nothing else is kept: no IP address, no user agent.
async function waitlist(request, env) {
  if (request.method !== 'POST') return new Response(null, { status: 405, headers: { Allow: 'POST' } });
  if (!sameSite(request)) return redirect('/waitlist-error');

  let form;
  try {
    form = new URLSearchParams(await readBody(request));
  } catch {
    return redirect('/waitlist-error');
  }
  // The hidden field is empty for people; bots fill it in. Pretend it worked.
  if (form.get('website')) return redirect('/waitlist');

  const email = (form.get('email') || '').trim().toLowerCase();
  if (email.length > 254 || !EMAIL.test(email)) return redirect('/waitlist-error');

  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(email));
  const id = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
  try {
    await env.WAITLIST.put(id, JSON.stringify({ email, joined: new Date().toISOString() }));
  } catch (err) {
    // Never log the address itself.
    console.error('waitlist: store failed:', err && err.name);
    return redirect('/waitlist-error');
  }
  return redirect('/waitlist');
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/api/waitlist') return waitlist(request, env);
    // One deployment answers both hosts, so the host decides which page an invite link gets.
    if (url.hostname === env.TESTNET_HOST && (url.pathname === '/join' || url.pathname === '/join/')) {
      url.pathname = '/join-testnet';
      return env.ASSETS.fetch(new Request(url, request));
    }
    return env.ASSETS.fetch(request);
  },
};

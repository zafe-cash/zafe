const MAX_BODY = 4096;
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

function sameSite(request) {
  const origin = request.headers.get('Origin');
  if (!origin) return false;
  try {
    return new URL(origin).host === new URL(request.url).host;
  } catch {
    return false;
  }
}

async function waitlist(request, env) {
  if (request.method !== 'POST') return new Response(null, { status: 405, headers: { Allow: 'POST' } });
  if (!sameSite(request)) return redirect('/waitlist-error');

  let form;
  try {
    form = new URLSearchParams(await readBody(request));
  } catch {
    return redirect('/waitlist-error');
  }
  if (form.get('website')) return redirect('/waitlist');

  const email = (form.get('email') || '').trim().toLowerCase();
  if (email.length > 254 || !EMAIL.test(email)) return redirect('/waitlist-error');

  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(email));
  const id = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
  try {
    await env.WAITLIST.put(id, JSON.stringify({ email, joined: new Date().toISOString() }));
  } catch (err) {
    console.error('waitlist: store failed:', err && err.name);
    return redirect('/waitlist-error');
  }
  return redirect('/waitlist');
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/api/waitlist') return waitlist(request, env);
    if (url.hostname === env.TESTNET_HOST && (url.pathname === '/join' || url.pathname === '/join/')) {
      url.pathname = '/join-testnet';
      return env.ASSETS.fetch(new Request(url, request));
    }
    return env.ASSETS.fetch(request);
  },
};

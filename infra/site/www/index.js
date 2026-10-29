// www.zafe.cash → zafe.cash, keeping the path and query (wrangler.jsonc next to this file).
// The main Worker can't do it: Cloudflare serves its static files without running code,
// whatever the host.
const APEX = 'zafe.cash';

export default {
  fetch(request) {
    const url = new URL(request.url);
    url.protocol = 'https:';
    url.hostname = APEX;
    url.port = '';
    return Response.redirect(url.toString(), 308);
  },
};

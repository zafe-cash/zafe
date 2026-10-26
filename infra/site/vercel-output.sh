#!/usr/bin/env bash
# Turns infra/site/dist (from build.sh) into Vercel's prebuilt output
# (.vercel/output, Build Output API v3) for `vercel deploy --prebuilt`, so Vercel serves
# exactly what build.sh made and never builds anything itself. Carries over what other
# hosts get from public/_headers: the same response headers, each page served at its
# clean path (/join from join.html, /showcase from showcase.html), and JSON content types
# for .well-known.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
dist="$here/dist"
output="$here/.vercel/output"

die() { echo "error: $*" >&2; exit 1; }

[[ -f "$dist/index.html" && -f "$dist/join.html" ]] || die "run build.sh first"

# One source for the headers: public/_headers ("/*" block, "  Name: value" lines).
headers_json="$(awk '
  /^\/\*$/ { block = 1; next }
  /^[^ ]/  { block = 0 }
  block && /^  [A-Za-z-]+: / {
    line = substr($0, 3); i = index(line, ": ")
    name = substr(line, 1, i - 1); value = substr(line, i + 2)
    gsub(/\\/, "\\\\", value); gsub(/"/, "\\\"", value)
    printf "%s\"%s\": \"%s\"", (n++ ? ", " : ""), name, value
  }
' "$here/public/_headers")"
[[ "$headers_json" == *Content-Security-Policy* ]] || die "no CSP found in public/_headers"

# Each "/assets/..." block (caching) becomes a route: "*" in the path matches anything.
asset_routes="$(python3 - "$here/public/_headers" <<'PY'
import json, re, sys
routes, path, headers = [], None, {}
def flush():
    if path and headers:
        src = '^' + re.escape(path).replace(r'\*', '(.*)') + '$'
        routes.append(json.dumps({'src': src, 'headers': headers, 'continue': True}))
for line in open(sys.argv[1]):
    if line.startswith('/'):
        flush()
        path, headers = (line.strip() if line.startswith('/assets/') else None), {}
    elif path and line.startswith('  ') and ': ' in line:
        name, value = line.strip().split(': ', 1)
        headers[name] = value
flush()
print(''.join(', ' + r for r in routes))
PY
)"

rm -rf "$output"
mkdir -p "$output"
cp -R "$dist" "$output/static"
rm -f "$output/static/_headers" "$output/static/_redirects" "$output/static/.nojekyll"

# Every page but the home page is served at its clean path (/join, /showcase, /docs,
# /docs/run-a-server: build.format 'file' writes join.html, docs/run-a-server.html), with
# /name/ and /name.html redirecting there.
overrides=''
redirects=''
shopt -s nullglob
for page in "$output"/static/*.html "$output"/static/docs/*.html; do
  name="${page#"$output/static/"}"
  name="${name%.html}"
  [[ "$name" == index || "$name" == relay ]] && continue
  [[ "$name" =~ ^[a-z0-9/-]+$ ]] || die "unexpected page name: $name"
  overrides+="${overrides:+, }\"$name.html\": { \"path\": \"$name\", \"contentType\": \"text/html; charset=utf-8\" }"
  redirects+="    { \"src\": \"^/$name/\$\", \"status\": 308, \"headers\": { \"Location\": \"/$name\" } },
    { \"src\": \"^/$name\\\\.html\$\", \"status\": 308, \"headers\": { \"Location\": \"/$name\" } },
"
done
shopt -u nullglob
# Permanent redirects (public/_redirects has the same for other hosts). /relay is linked
# from the app's first "Run your own server" link (never released): keep it as a redirect.
redirects+="    { \"src\": \"^/relay(/|\\\\.html)?\$\", \"status\": 308, \"headers\": { \"Location\": \"/docs/run-a-server\" } },
"
for f in assetlinks.json apple-app-site-association; do
  if [[ -f "$output/static/.well-known/$f" ]]; then
    overrides+=", \".well-known/$f\": { \"contentType\": \"application/json\" }"
  fi
done

# The testnet host serves the testnet invite page (join-testnet.html) at /join: the same
# site and deployment answers both hosts, so the host decides which page an invite link gets.
testnet_host="${ZAFE_TESTNET_HOST:-testnet.zafe.cash}"
[[ "$testnet_host" =~ ^[a-z0-9.-]+$ ]] || die "ZAFE_TESTNET_HOST must be a bare lower-case host"
testnet_host_re="${testnet_host//./\\\\.}"

# The one function: the iPhone waitlist (api/waitlist.mjs), bundled with its dependency into
# a single file, served at /api/waitlist (a Node function in the Build Output API).
func="$output/functions/api/waitlist.func"
mkdir -p "$func"
"$here/node_modules/.bin/esbuild" "$here/api/waitlist.mjs" --bundle --platform=node \
  --target=node22 --format=esm --outfile="$func/index.mjs" --log-level=warning \
  --banner:js="import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);"
cat > "$func/.vc-config.json" <<'JSON'
{ "runtime": "nodejs22.x", "handler": "index.mjs", "launcherType": "Nodejs", "shouldAddHelpers": false, "maxDuration": 10 }
JSON

cat > "$output/config.json" <<EOF
{
  "version": 3,
  "routes": [
$redirects    { "src": "^/(.*)$", "headers": { $headers_json }, "continue": true }$asset_routes,
    { "src": "^/\\\\.well-known/(.*)$", "headers": { "Cache-Control": "public, max-age=300" }, "continue": true },
    { "src": "^/join/?$", "has": [{ "type": "host", "value": "$testnet_host_re" }], "dest": "/join-testnet" },
    { "handle": "filesystem" },
    { "handle": "error" },
    { "status": 404, "src": "^/(.*)$", "dest": "/404" }
  ],
  "overrides": { $overrides }
}
EOF

python3 -m json.tool "$output/config.json" > /dev/null || die "invalid config.json"
echo "Wrote $output"

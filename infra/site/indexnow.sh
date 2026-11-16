#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
die() { echo "indexnow: $*" >&2; exit 1; }

shopt -s nullglob
keys=("$here"/public/[0-9a-f]*.txt)
(( ${#keys[@]} == 1 )) || die "expected exactly one public/<key>.txt, found ${#keys[@]}"
key="$(basename "${keys[0]}" .txt)"
[[ "$key" =~ ^[0-9a-f]{32}$ ]] || die "bad key file name: $key"
[[ "$(tr -d '\n' < "${keys[0]}")" == "$key" ]] || die "public/$key.txt must contain the key"

sitemap="$here/dist/sitemap.xml"
[[ -f "$sitemap" ]] || die "no $sitemap: run build.sh first"
mapfile -t urls < <(grep -o '<loc>[^<]*</loc>' "$sitemap" | sed 's/<\/\?loc>//g')
(( ${#urls[@]} )) || die "no URLs in the sitemap"
host="$(sed -E 's#^https://([^/]+)/.*#\1#' <<< "${urls[0]}")"

body="$(python3 - "$host" "$key" "${urls[@]}" <<'PY'
import json, sys
host, key, *urls = sys.argv[1:]
print(json.dumps({"host": host, "key": key,
                  "keyLocation": f"https://{host}/{key}.txt", "urlList": urls}))
PY
)"
status="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 30 \
  -H 'Content-Type: application/json; charset=utf-8' \
  --data "$body" https://api.indexnow.org/indexnow)"
[[ "$status" == 200 || "$status" == 202 ]] || die "HTTP $status for ${#urls[@]} URLs"
echo "IndexNow: submitted ${#urls[@]} URLs for $host (HTTP $status)"

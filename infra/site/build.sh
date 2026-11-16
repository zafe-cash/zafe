#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="$here/dist"
packages=(xyz.zafe.zafe xyz.zafe.zafe.testnet)
repo="https://github.com/zafe-cash/zafe"
download="${ZAFE_DOWNLOAD_URL:-}"
testnet="${ZAFE_TESTNET_URL:-}"
source_url="${ZAFE_SOURCE_URL:-$repo}"

die() { echo "error: $*" >&2; exit 1; }

plain_https() {
  [[ "$1" =~ ^https://[A-Za-z0-9._~:/?#@!\$\&\(\)*+,\;=%-]+$ && "$1" != *"'"* ]]
}

[[ -z "$download" ]] || plain_https "$download" || die "ZAFE_DOWNLOAD_URL must be a plain https:// URL"
[[ -z "$testnet" ]] || plain_https "$testnet" || die "ZAFE_TESTNET_URL must be a plain https:// URL"
plain_https "$source_url" || die "ZAFE_SOURCE_URL must be a plain https:// URL"

fingerprints=()
if [[ -n "${ZAFE_ANDROID_CERT_SHA256:-}" ]]; then
  IFS=',' read -ra raw <<< "$ZAFE_ANDROID_CERT_SHA256"
  for f in "${raw[@]}"; do
    hex="$(tr -d ': \t' <<< "$f" | tr 'a-f' 'A-F')"
    [[ "$hex" =~ ^[0-9A-F]{64}$ ]] || die "not a SHA-256 fingerprint: '$f'"
    fingerprints+=("$(sed 's/../&:/g; s/:$//' <<< "$hex")")
  done
fi

app_ids=()
if [[ -n "${ZAFE_IOS_APP_IDS:-}" ]]; then
  IFS=',' read -ra raw <<< "$ZAFE_IOS_APP_IDS"
  for id in "${raw[@]}"; do
    id="$(tr -d ' \t' <<< "$id")"
    [[ "$id" =~ ^[A-Z0-9]{10}\.[A-Za-z0-9.-]+$ ]] || die "not an iOS app ID (<TeamID>.<bundle>): '$id'"
    app_ids+=("$id")
  done
fi

join_quoted() { local IFS=,; local q=(); for v in "$@"; do q+=("\"$v\""); done; echo "${q[*]}"; }

cd "$here"
[[ -d node_modules ]] || npm ci --no-audit --no-fund
ZAFE_DOWNLOAD_URL="$download" ZAFE_TESTNET_URL="$testnet" ZAFE_SOURCE_URL="$source_url" ASTRO_TELEMETRY_DISABLED=1 \
  npx --no-install astro build --silent
touch "$out/.nojekyll"

shopt -s nullglob
pages=("$out"/*.html "$out"/docs/*.html)
shopt -u nullglob
if grep -l -P '<script(?![^>]*\ssrc=)(?!\s+type="?application/ld\+json"?>)[^>]*>|<style|\s(style|on[a-z]+)=' "${pages[@]}"; then
  die "inline script or style in the pages above (the CSP would block it)"
fi
python3 - "${pages[@]}" <<'PY' || die "invalid JSON-LD"
import json, re, sys
for page in sys.argv[1:]:
    html = open(page, encoding='utf-8').read()
    for block in re.findall(r'<script\s+type="?application/ld\+json"?>(.*?)</script>', html, re.S):
        json.loads(block)
PY

if (( ${#fingerprints[@]} )); then
  mkdir -p "$out/.well-known"
  python3 - "$out/.well-known/assetlinks.json" "${fingerprints[@]}" -- "${packages[@]}" <<'PY'
import json, sys
args = sys.argv[2:]
cut = args.index("--")
prints, packages = args[:cut], args[cut + 1:]
links = [{
    "relation": ["delegate_permission/common.handle_all_urls"],
    "target": {"namespace": "android_app", "package_name": p, "sha256_cert_fingerprints": prints},
} for p in packages]
with open(sys.argv[1], "w") as f:
    json.dump(links, f, indent=2)
    f.write("\n")
PY
else
  echo "warning: no ZAFE_ANDROID_CERT_SHA256, so no assetlinks.json (App Links won't verify)" >&2
fi

if (( ${#app_ids[@]} )); then
  mkdir -p "$out/.well-known"
  cat > "$out/.well-known/apple-app-site-association" <<EOF
{
  "applinks": {
    "details": [
      {
        "appIDs": [$(join_quoted "${app_ids[@]}")],
        "components": [
          { "/": "/join", "comment": "Invite links" },
          { "/": "/join/", "comment": "Invite links" }
        ]
      }
    ]
  }
}
EOF
fi

if command -v python3 > /dev/null; then
  for f in "$out"/.well-known/{assetlinks.json,apple-app-site-association}; do
    [[ -e "$f" ]] || continue
    python3 -m json.tool "$f" > /dev/null || die "invalid JSON: $f"
  done
fi

echo "Built $out"
echo "  Android certificates: ${#fingerprints[@]}"
echo "  iOS app IDs: ${#app_ids[@]}${app_ids[*]:+ (${app_ids[*]})}"
echo "  Download: $download"

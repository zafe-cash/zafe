#!/usr/bin/env bash
# Builds the site (home page, and the invite page https://<ZAFE_LINK_HOST>/join#<invite>) into
# infra/site/dist, ready to upload to any static host. See README.md.
#
#   ZAFE_ANDROID_CERT_SHA256   SHA-256 fingerprints of the APK signing certificates,
#                              comma-separated (AA:BB:... or plain hex). Without them
#                              there is no assetlinks.json, so App Links can't verify.
#   ZAFE_IOS_APP_IDS           optional: <TeamID>.xyz.zafe.zafe, comma-separated
#   ZAFE_DOWNLOAD_URL          optional: where "Get Zafe" points. Unset until there is an
#                              APK testers can use: the hero then links to the source and
#                              the footer has no Android link
#   ZAFE_SOURCE_URL            optional: where "Read the source" points
#                              (default: the GitHub repository)
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="$here/dist"
# The mainnet app and the separate testnet app (ZAFE_NETWORK=test builds).
packages=(xyz.zafe.zafe xyz.zafe.zafe.testnet)
repo="https://github.com/zafe-cash/zafe"
download="${ZAFE_DOWNLOAD_URL:-}"
source_url="${ZAFE_SOURCE_URL:-$repo}"

die() { echo "error: $*" >&2; exit 1; }

plain_https() {
  [[ "$1" =~ ^https://[A-Za-z0-9._~:/?#@!\$\&\(\)*+,\;=%-]+$ && "$1" != *"'"* ]]
}

[[ -z "$download" ]] || plain_https "$download" || die "ZAFE_DOWNLOAD_URL must be a plain https:// URL"
plain_https "$source_url" || die "ZAFE_SOURCE_URL must be a plain https:// URL"

# Fingerprints → "AA:BB:...", upper case, exactly 32 bytes.
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
ZAFE_DOWNLOAD_URL="$download" ZAFE_SOURCE_URL="$source_url" ASTRO_TELEMETRY_DISABLED=1 \
  npx --no-install astro build --silent
touch "$out/.nojekyll" # GitHub Pages: serve .well-known

# The CSP allows only same-origin files: refuse a build that inlined any script or style.
# A <script> must have a src (Astro's bundled modules do); anything else is inline.
# One exception: a <script> whose only attribute is type="application/ld+json" (schema.org
# data for search engines, Base.astro). Browsers never execute that type, so script-src
# doesn't apply to it; any other attribute or type still fails, and its contents must
# parse as JSON (checked below).
if grep -l -P '<script(?![^>]*\ssrc=)(?!\s+type="?application/ld\+json"?>)[^>]*>|<style|\s(style|on[a-z]+)=' "$out"/*.html; then
  die "inline script or style in the pages above (the CSP would block it)"
fi
python3 - "$out"/*.html <<'PY' || die "invalid JSON-LD"
import json, re, sys
for page in sys.argv[1:]:
    html = open(page, encoding='utf-8').read()
    for block in re.findall(r'<script\s+type="?application/ld\+json"?>(.*?)</script>', html, re.S):
        json.loads(block)
PY

if (( ${#fingerprints[@]} )); then
  mkdir -p "$out/.well-known"
  # One entry per app package, each with every signing certificate.
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

# Universal Links, only for the join path (the invite is in the fragment).
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

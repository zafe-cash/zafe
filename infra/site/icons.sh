#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
svg="$here/public/assets/zafe.svg"
ios="$here/../../app/ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png"
icons="$here/public/assets/icons"
mkdir -p "$icons"

tile() { convert -background none -density 96 "$svg" -resize "$1x$1" -strip "PNG32:$2"; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
for s in 16 32 48; do tile "$s" "$tmp/$s.png"; done
convert "$tmp/16.png" "$tmp/32.png" "$tmp/48.png" "$here/public/favicon.ico"

tile 192 "$icons/icon-192.png"
tile 512 "$icons/icon-512.png"
convert "$ios" -resize 512x512 -strip "PNG24:$icons/maskable-512.png"
convert "$ios" -resize 180x180 -strip "PNG24:$here/public/apple-touch-icon.png"
echo "Icons written"

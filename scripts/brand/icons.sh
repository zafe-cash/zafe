#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
python3 "$here/brand.py"
[[ -f ~/android/env.sh ]] && source ~/android/env.sh
cd "$root/app"
flutter test tool/brand/render_test.dart --reporter compact
if command -v convert >/dev/null; then
  for f in ios/Runner/Assets.xcassets/AppIcon.appiconset/*.png; do
    convert "$f" -background '#004A46' -alpha remove -alpha off "$f"
  done
else
  echo "warning: ImageMagick not found; iOS icons keep an (opaque) alpha channel" >&2
fi
echo "Preview: $root/app/build/brand_preview/brand_sheet.png"

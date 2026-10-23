#!/usr/bin/env bash
# Gathers what the loops are made of into public/ (gitignored): the app's cards (from
# site_motion_render_test) and the rendered scenes (from scenes/render.sh). Run after both:
#
#   cd app && flutter test --dart-define=ZAFE_NETWORK=main tool/screens/site_motion_render_test.dart
#   BLENDER=... infra/site/scenes/render.sh <workdir>
#   infra/site/film/prepare.sh <workdir>
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
work="${1:?usage: prepare.sh <scenes workdir>}"
cards="$here/../../../app/build/screen_preview"
pub="$here/public"
rm -rf "$pub/frames" "$pub/gen" && mkdir -p "$pub/frames" "$pub/gen"
for c in request_card send_card keys_card vault_card_dark; do cp -r "$cards/motion_$c" "$pub/frames/$c"; done
for s in sign sent setup live; do
  for tag in "$s" "${s}_tall"; do cp -r "$work/out/$tag" "$pub/gen/$tag"; done
done
for d in "$pub/frames"/*/ "$pub/gen"/*/; do echo "$(basename "$d") $(ls "$d" | wc -l)"; done

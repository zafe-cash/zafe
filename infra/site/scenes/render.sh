#!/usr/bin/env bash
# Renders the site's illustrated loops (docs/site.md §6): the textures, then each scene wide
# (1600x900) and tall (720x1280) into <workdir>/out/<scene>[_tall]/f_0001.png ...
# A few Blender processes share each loop's frames (kit.render's placeholders); JOBS caps how
# many run at once (each takes about 1 GB of GPU memory).
#
#   BLENDER=/path/to/blender scenes/render.sh <workdir> [scene ...]   # default: all four
#
# On WSL with Blender for Windows, point BLENDER at blender.exe and use a <workdir> on the
# Windows side (/mnt/c/...): the scripts are copied there and paths are passed as Windows paths.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
work="${1:?usage: render.sh <workdir> [scene ...]}"
shift
scenes=("$@")
[ ${#scenes[@]} -eq 0 ] && scenes=(sign sent setup live)
BLENDER="${BLENDER:-blender}"
JOBS="${JOBS:-6}"
PER="${PER:-2}"  # processes per loop

mkdir -p "$work/out"
cp "$here"/{kit,props,tex,sign,sent,setup,live}.py "$work/"
python3 "$here/tex.py" "$work/tex" >/dev/null

native() { if [[ "$BLENDER" == *.exe ]]; then wslpath -w "$1"; else echo "$1"; fi; }
export BLENDER WSLENV="ZAFE_TALL:ZAFE_TAG"
# one line per Blender process: <tall> TAB <tag> TAB <script> TAB <workdir> (paths may have spaces)
for scene in "${scenes[@]}"; do
  for tall in 0 1; do
    tag="$scene"
    [ "$tall" = 1 ] && tag="${scene}_tall"
    rm -rf "$work/out/$tag"
    for _ in $(seq "$PER"); do
      printf '%s\t%s\t%s\t%s\n' "$tall" "$tag" "$(native "$work/$scene.py")" "$(native "$work")"
    done
  done
done | xargs -d '\n' -n 1 -P "$JOBS" bash -c '
  IFS=$'"'"'\t'"'"' read -r tall tag script wd <<< "$1"
  sleep "$((RANDOM % 3))"  # stagger starts so placeholders spread the frames
  ZAFE_TALL=$tall ZAFE_TAG=$tag "$BLENDER" --background --factory-startup --python "$script" -- "$wd" anim 2>&1 |
    tr -d "\r" | { grep -E "Traceback|Error:" || true; } | sed "s/^/$tag: /"' _

for scene in "${scenes[@]}"; do
  for tag in "$scene" "${scene}_tall"; do
    n=$(find "$work/out/$tag" -name 'f_*.png' -size +0 | wc -l)
    echo "$tag: $n frames"
  done
done

#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
work="${1:?usage: render.sh <workdir> [scene ...]}"
shift
scenes=("$@")
[ ${#scenes[@]} -eq 0 ] && scenes=(sign sent setup live)
BLENDER="${BLENDER:-blender}"
JOBS="${JOBS:-6}"
PER="${PER:-2}"

mkdir -p "$work/out"
cp "$here"/{kit,props,tex,sign,sent,setup,live}.py "$work/"
python3 "$here/tex.py" "$work/tex" >/dev/null

native() { if [[ "$BLENDER" == *.exe ]]; then wslpath -w "$1"; else echo "$1"; fi; }
export BLENDER WSLENV="ZAFE_TALL:ZAFE_TAG"
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

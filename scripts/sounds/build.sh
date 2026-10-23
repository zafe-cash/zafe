#!/usr/bin/env bash
# Regenerates the payment sounds and writes the app's copies (Ogg Vorbis, which
# Android's SoundPool plays natively, and AAC .m4a in app/assets/sounds for iOS). Needs python3 and ffmpeg with libvorbis.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 "$root/scripts/sounds/sounds.py" "$tmp"
raw="$root/app/android/app/src/main/res/raw"
for wav in "$tmp"/*.wav; do
  name="$(basename "$wav" .wav)"
  ffmpeg -v error -y -i "$wav" -c:a libvorbis -q:a 5 -map_metadata -1 "$raw/$name.ogg"
done
ios="$root/app/assets/sounds"
mkdir -p "$ios"
for wav in "$tmp"/*.wav; do
  name="$(basename "$wav" .wav)"
  ffmpeg -v error -y -i "$wav" -c:a aac -b:a 64k -map_metadata -1 "$ios/$name.m4a"
done
ls -l "$raw"/pay_*.ogg "$ios"/pay_*.m4a

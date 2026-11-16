#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/app/build/illustration_device}"
pkg=xyz.zafe.zafe
mkdir -p "$out"
[[ -f ~/android/env.sh ]] && source ~/android/env.sh

if [[ "${2:-}" != "--no-build" ]]; then
  (cd "$root/app" && flutter build apk --debug --target-platform android-x64)
  adb install -r "$root/app/build/app/outputs/flutter-apk/app-debug.apk"
fi

shoot() {
  local theme=$1
  adb shell cmd uimode night "$([[ $theme == dark ]] && echo yes || echo no)" >/dev/null
  adb shell pm clear "$pkg" >/dev/null
  agent-device close >/dev/null 2>&1 || true
  agent-device open "$pkg" --foreground >/dev/null
  agent-device wait text "Create a vault" >/dev/null
  sleep 1
  agent-device screenshot "$out/welcome_$theme.png"
  agent-device find "Create a vault" click >/dev/null
  agent-device wait text "Vault name" >/dev/null
  sleep 1
  agent-device screenshot "$out/create_$theme.png"
  adb shell am force-stop "$pkg"
  agent-device close >/dev/null 2>&1 || true
  agent-device open "$pkg" --foreground >/dev/null
  agent-device wait text "Join with an invite" >/dev/null
  agent-device find "Join with an invite" click >/dev/null
  agent-device wait text "Paste" >/dev/null
  sleep 1
  agent-device screenshot "$out/join_$theme.png"
  agent-device close >/dev/null
}

shoot light
shoot dark
adb shell cmd uimode night no >/dev/null
echo "Screenshots in $out"

#!/usr/bin/env bash
# Installs the CI's unsigned iOS build on an iPhone from Linux with xtool (xtool.sh):
# xtool signs it with your Apple ID through Apple Developer Services and installs it over
# USB. Needs `xtool setup` done once (its Darwin SDK, `xtool auth login`), usbmuxd running,
# the phone unlocked, trusted and in Developer Mode, and `gh` logged in for the download.
#
#   scripts/ios-install.sh [run-id]        # default: the latest successful iOS build on this branch
#
# Not verified on a phone yet (see docs/tracker.md): xtool doesn't support most
# entitlements or app extensions, and a free Apple ID's signature lasts 7 days.
set -euo pipefail
command -v xtool >/dev/null || { echo "xtool is not installed (https://xtool.sh)"; exit 1; }
command -v gh >/dev/null || { echo "gh is not installed"; exit 1; }
run="${1:-}"
if [ -z "$run" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD)"
  run="$(gh run list --workflow=ios.yml --branch "$branch" --status success --limit 1 \
    --json databaseId --jq '.[0].databaseId')"
fi
[ -n "$run" ] || { echo "no successful iOS build found"; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
gh run download "$run" --name zafe-ios-unsigned --dir "$tmp"
(cd "$tmp" && unzip -q Zafe-unsigned.app.zip && mkdir Payload && mv Runner.app Payload/ \
  && zip -qry Zafe.ipa Payload)
xtool install "$tmp/Zafe.ipa"

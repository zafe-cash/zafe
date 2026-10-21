#!/usr/bin/env bash
# Stops and deletes the ths environment started by up.sh (and nothing else).
set -uo pipefail
THS=${THS:-ths}
NAME=${ZAFE_REGTEST_NAME:-zafe-regtest}
STATE=${ZAFE_REGTEST_STATE:-$HOME/.cache/zafe-regtest}
PID_FILE="$STATE/$NAME.pid"

# Ctrl+C is how `ths start` expects to be stopped: it deletes the environment and exits.
# (`ths stop` also ends the launcher since 0.4.0; the fallback below catches the rest.)
if [[ -f "$PID_FILE" ]]; then
  pid=$(cat "$PID_FILE")
  if kill -INT "$pid" 2>/dev/null; then
    for _ in $(seq 60); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
    kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null
  fi
  rm -f "$PID_FILE"
fi
# Whatever is left (no launcher, or one that was killed): ths removes it by name.
"$THS" --name "$NAME" stop >/dev/null 2>&1
exit 0

#!/usr/bin/env bash
set -uo pipefail
THS=${THS:-ths}
NAME=${ZAFE_REGTEST_NAME:-zafe-regtest}
STATE=${ZAFE_REGTEST_STATE:-$HOME/.cache/zafe-regtest}
PID_FILE="$STATE/$NAME.pid"

if [[ -f "$PID_FILE" ]]; then
  pid=$(cat "$PID_FILE")
  if kill -INT "$pid" 2>/dev/null; then
    for _ in $(seq 60); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
    kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null
  fi
  rm -f "$PID_FILE"
fi
"$THS" --name "$NAME" stop >/dev/null 2>&1
exit 0

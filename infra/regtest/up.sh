#!/usr/bin/env bash
# Starts a disposable Ironwood regtest network with `ths` (thus-spoke-zakura): a Zakura node,
# lightwalletd and ths's own wallet, which mines and runs the faucet. Fund addresses with
# fund.sh; mine with `ths --name <name> mine N`.
#   infra/regtest/up.sh
# Env: ZAFE_REGTEST_NAME (default zafe-regtest), ZAFE_REGTEST_PORT_OFFSET (default 0, a
#      multiple of 10, at most 32730): Zakura RPC on 18232 + offset, lightwalletd on
#      9067 + offset, ths's dashboard on 32805 + offset. Everything binds to 127.0.0.1.
#      THS (default `ths` on PATH) must be version THS_VERSION below.
set -euo pipefail
THS_VERSION=0.4.0
THS=${THS:-ths}
NAME=${ZAFE_REGTEST_NAME:-zafe-regtest}
OFFSET=${ZAFE_REGTEST_PORT_OFFSET:-0}
DIR=$(cd "$(dirname "$0")" && pwd)
STATE=${ZAFE_REGTEST_STATE:-$HOME/.cache/zafe-regtest}

found=$("$THS" --version 2>/dev/null | awk '{print $2}') || true
if [[ "$found" != "$THS_VERSION" ]]; then
  echo "need ths $THS_VERSION (found: ${found:-none}). Install it with:" >&2
  echo "  curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/zcashlabs/thus-spoke-zakura/main/install.sh | THS_VERSION=$THS_VERSION sh" >&2
  exit 1
fi

"$DIR/down.sh" >/dev/null 2>&1 || true
mkdir -p "$STATE"
LOG="$STATE/$NAME.log"
# `ths start` stays in the foreground and deletes the environment when interrupted; run it
# in its own session so it outlives this script, and keep its pid for down.sh.
setsid nohup "$THS" --name "$NAME" start --no-open --port-offset "$OFFSET" > "$LOG" 2>&1 < /dev/null &
echo $! > "$STATE/$NAME.pid"

# The first start pulls the runtime images.
for _ in $(seq 600); do
  grep -q "is ready" "$LOG" && break
  if ! kill -0 "$(cat "$STATE/$NAME.pid")" 2>/dev/null; then
    echo "ths failed to start $NAME:" >&2; tail -20 "$LOG" >&2; exit 1
  fi
  sleep 0.5
done
grep -q "is ready" "$LOG" || { echo "ths did not start $NAME in time:" >&2; tail -20 "$LOG" >&2; exit 1; }
"$THS" --name "$NAME" endpoints

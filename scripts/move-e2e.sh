#!/usr/bin/env bash
# Relay move end to end (docs/relay-move.md §4.2-4.5, §11): three `zafe` processes on a
# 2-of-3 vault move it from relay 1 to relay 2. A payment approved by one member on relay 1
# is finished on relay 2 after the switch, and a new payment goes through relay 2 alone.
#
# Needs Docker and `ths` (see infra/regtest/up.sh). Run from the repository root:
# scripts/move-e2e.sh
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/zafe-move.XXXXXX")
OLD_PORT=${ZAFE_MOVE_OLD_PORT:-48797}
NEW_PORT=${ZAFE_MOVE_NEW_PORT:-48798}
export ZAFE_REGTEST_NAME=zafe-move ZAFE_REGTEST_PORT_OFFSET=30300
export ZAFE_RELAY="http://127.0.0.1:$OLD_PORT" ZAFE_LIGHTWALLETD="http://127.0.0.1:$((9067 + ZAFE_REGTEST_PORT_OFFSET))"
NEW_RELAY="http://127.0.0.1:$NEW_PORT"

cargo build -q -p zafe-cli -p zafe-relay
cargo build -q -p zafe-core --example vault_address
ZAFE="$ROOT/target/debug/zafe"

cleanup() {
  for pid in ${OLD_PID:-} ${NEW_PID:-}; do kill "$pid" 2>/dev/null || true; done
  "$ROOT/infra/regtest/down.sh" || true
  rm -rf "$WORK"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
member() { local who=$1; shift; "$ZAFE" --home "$WORK/$who" "$@"; }
mine() { "${THS:-ths}" --name "$ZAFE_REGTEST_NAME" mine "$1" >/dev/null; }
relay() {  # relay <port> <db>: start a relay, wait until it accepts connections
  ZAFE_RELAY_LISTEN="127.0.0.1:$1" ZAFE_RELAY_DB="$2" "$ROOT/target/debug/zafe-relay" >"$2.log" 2>&1 &
  local pid=$!
  for _ in $(seq 50); do
    (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null && break
    sleep 0.2
  done
  echo "$pid"
}
wait_lwd() {
  for _ in $(seq 120); do
    h=$(member A sync 2>/dev/null | awk '{print $2}') || true
    [[ "${h:-0}" -ge "$1" ]] && return 0
    sleep 1
  done
  echo "lightwalletd did not reach height $1" >&2; exit 1
}

step "two relays"
OLD_PID=$(relay "$OLD_PORT" "$WORK/old.sqlite")
NEW_PID=$(relay "$NEW_PORT" "$WORK/new.sqlite")

step "A, B and C make a 2-of-3 vault on relay 1"
for who in A B C; do member "$who" init; done
INVITE=$(member A vault create --name Grants --threshold 2 --members 3)
member B vault join "$INVITE"
member C vault join "$INVITE"
member A vault seal
pids=()
for who in A B C; do member "$who" vault members --wait > "$WORK/m$who" & pids+=($!); done
for pid in "${pids[@]}"; do wait "$pid"; done
SN=$(sed -n 's/^safety words: //p' "$WORK/mA")
member A vault keygen --safety-number "$SN" --birthday 2 > "$WORK/kA" &
KA=$!
member B vault keygen --safety-number "$SN" > "$WORK/kB" &
KB=$!
member C vault keygen --safety-number "$SN" > "$WORK/kC" &
KC=$!
wait "$KA" "$KB" "$KC"
ADDR=$(member A vault show | sed -n 's/^address //p')
# Every app announces itself (keygen did); a refresh makes each copy current.
for who in A B C; do member "$who" proposals >/dev/null; done

step "regtest chain; the faucet funds the vault"
"$ROOT/infra/regtest/up.sh"
"$ROOT/infra/regtest/fund.sh" "$ADDR" 3
TIP=$(curl -sf -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getblockcount","params":[]}' \
  "http://127.0.0.1:$((18232 + ZAFE_REGTEST_PORT_OFFSET))" | sed 's/.*"result":\([0-9]*\).*/\1/')
wait_lwd "$TIP"
for who in A B C; do member "$who" sync >/dev/null; done
RECIPIENT=$("$ROOT/target/debug/examples/vault_address" | sed -n 's/^ua=//p')

step "a payment starts on relay 1: A proposes, B approves (one tap; C approves later)"
P1=$(member A propose --to "$RECIPIENT" --amount 100000000 --memo "Before the move" | awk '{print $2}')
member B approve "$P1"

step "A moves the vault to relay 2"
LINK=$(member A move "$NEW_RELAY")
echo "$LINK"
[[ "$LINK" == https://zafe.cash/move#* ]] || { echo "no move link" >&2; exit 1; }
member A move --status

step "B sees the notice on relay 1, follows: 2 of 3 have moved, the vault switches"
member B move --status | grep -q "move notice" || { echo "B has no move notice" >&2; exit 1; }
member B follow "$LINK" | tee "$WORK/fB"
grep -q "switched" "$WORK/fB" || { echo "the vault didn't switch at 2 of 3" >&2; exit 1; }

step "C was away: the notice says the vault moved, so C can't write to relay 1"
member C move --status | tee "$WORK/sC"
grep -q "switched" "$WORK/sC" || { echo "C doesn't see the switch" >&2; exit 1; }
if member C approve "$P1" 2>"$WORK/eC"; then
  echo "C wrote to relay 1 after the switch" >&2; exit 1
fi
grep -q "moved" "$WORK/eC" || { cat "$WORK/eC" >&2; exit 1; }
member C follow "$LINK"

step "the payment started on relay 1 completes on relay 2"
member C approve "$P1"
member C send "$P1"
mine 2
sleep 3
member A move --status
member A proposals | grep "$P1" | grep -q Broadcast || { echo "P1 not broadcast" >&2; exit 1; }
member A vault show | grep -q "relay $NEW_RELAY" || { echo "A's vault didn't follow" >&2; exit 1; }

step "a new payment after the switch goes through relay 2 only"
for who in A B C; do member "$who" sync >/dev/null; done
P2=$(member A propose --to "$RECIPIENT" --amount 50000000 --memo "After the move" | awk '{print $2}')
member B approve "$P2"
member C approve "$P2"
member A send "$P2"
mine 2
sleep 3
member B proposals | grep "$P2" | grep -q Broadcast || { echo "P2 not broadcast" >&2; exit 1; }

step "relay 1 got nothing after the switch: stop it, everything still works"
kill "$OLD_PID"; OLD_PID=
member B proposals >/dev/null
echo
echo "Relay move end-to-end: OK"

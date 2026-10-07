#!/usr/bin/env bash
# M0 acceptance test: three separate `zafe` processes (members A, B, C) create a 2-of-3
# vault through the relay, the vault is funded on a local Ironwood regtest chain, A
# proposes a payment, B and C verify, approve and sign, and A broadcasts it.
#
# Needs Docker and `ths` (see infra/regtest/up.sh). Run from the repository root: scripts/m0-e2e.sh
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/zafe-m0.XXXXXX")
RELAY_PORT=${ZAFE_M0_RELAY_PORT:-48787}
# ths moves every port by the offset: lightwalletd on 39267.
export ZAFE_REGTEST_NAME=zafe-m0 ZAFE_REGTEST_PORT_OFFSET=30200
export ZAFE_RELAY="http://127.0.0.1:$RELAY_PORT" ZAFE_LIGHTWALLETD="http://127.0.0.1:$((9067 + ZAFE_REGTEST_PORT_OFFSET))"

cargo build -q -p zafe-cli -p zafe-relay
cargo build -q -p zafe-core --example vault_address
ZAFE="$ROOT/target/debug/zafe"

cleanup() {
  [[ -n "${RELAY_PID:-}" ]] && kill "$RELAY_PID" 2>/dev/null || true
  "$ROOT/infra/regtest/down.sh" || true
  rm -rf "$WORK"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
member() { local who=$1; shift; "$ZAFE" --home "$WORK/$who" "$@"; }
mine() { "${THS:-ths}" --name "$ZAFE_REGTEST_NAME" mine "$1" >/dev/null; }
wait_lwd() {  # wait until lightwalletd serves height $1
  for _ in $(seq 120); do
    h=$(member A sync 2>/dev/null | awk '{print $2}') || true
    [[ "${h:-0}" -ge "$1" ]] && return 0
    sleep 1
  done
  echo "lightwalletd did not reach height $1" >&2; exit 1
}

step "relay"
ZAFE_RELAY_LISTEN="127.0.0.1:$RELAY_PORT" ZAFE_RELAY_DB="$WORK/relay.sqlite" "$ROOT/target/debug/zafe-relay" &
RELAY_PID=$!
for _ in $(seq 50); do  # wait until the relay accepts connections
  (exec 3<>"/dev/tcp/127.0.0.1/$RELAY_PORT") 2>/dev/null && break
  sleep 0.2
done

step "identities"
for who in A B C; do member "$who" init; done

step "A creates a 2-of-3 vault; B and C join with the invite"
INVITE=$(member A vault create --name Grants --threshold 2 --members 3)
member B vault join "$INVITE"
member C vault join "$INVITE"
member A vault seal

step "everyone compares the safety number"
SN_A=$(member A vault members | sed -n 's/^safety number: //p')
SN_B=$(member B vault members | sed -n 's/^safety number: //p')
SN_C=$(member C vault members | sed -n 's/^safety number: //p')
echo "A: $SN_A  B: $SN_B  C: $SN_C"
[[ "$SN_A" == "$SN_B" && "$SN_B" == "$SN_C" ]] || { echo "safety numbers differ" >&2; exit 1; }

step "key generation (three processes, through the relay)"
member A vault keygen --safety-number "$SN_A" --birthday 2 > "$WORK/kA" &
KA=$!
member B vault keygen --safety-number "$SN_B" > "$WORK/kB" &
KB=$!
member C vault keygen --safety-number "$SN_C" > "$WORK/kC" &
KC=$!
wait "$KA" "$KB" "$KC"
cat "$WORK/kA" "$WORK/kB" "$WORK/kC"
ADDR=$(member A vault show | sed -n 's/^address //p')
[[ "$(member B vault show | sed -n 's/^address //p')" == "$ADDR" ]]
[[ "$(member C vault show | sed -n 's/^address //p')" == "$ADDR" ]]

step "regtest chain (ths); the faucet funds the vault"
"$ROOT/infra/regtest/up.sh"
"$ROOT/infra/regtest/fund.sh" "$ADDR" 3
TIP=$(curl -sf -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getblockcount","params":[]}' \
  "http://127.0.0.1:$((18232 + ZAFE_REGTEST_PORT_OFFSET))" | sed 's/.*"result":\([0-9]*\).*/\1/')
wait_lwd "$TIP"
for who in A B C; do echo "$who: $(member "$who" sync)"; done

step "A proposes 1 ZEC to an outside recipient"
RECIPIENT=$("$ROOT/target/debug/examples/vault_address" | sed -n 's/^ua=//p')
PROPOSAL=$(member A propose --to "$RECIPIENT" --amount 100000000 --memo "Grant milestone 1" | awk '{print $2}')
member A proposals

step "B and C verify independently and approve"
member B approve "$PROPOSAL"
member C approve "$PROPOSAL"
member A proposals

# Keygen published every member's commitment pool, so the approvals above already carry
# the signature shares (one tap); anyone can aggregate and broadcast. The interactive
# request/respond/finalize path is covered by `bridge_e2e`.
step "A aggregates the approvals, proves and broadcasts"
member A send "$PROPOSAL"
mine 2
sleep 3
member A proposals
member B sync
member A proposals | grep -q "Broadcast" || { echo "proposal not broadcast" >&2; exit 1; }
echo
echo "M0 end-to-end: OK"

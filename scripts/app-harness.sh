#!/usr/bin/env bash
# Device-testing harness for the app: a relay and headless CLI members (B, C, ...) on this
# machine; the app on a phone/emulator is the last member. `adb reverse` lets the device
# reach the relay (8787) and lightwalletd (9067) at 127.0.0.1, matching the app defaults.
#
#   scripts/app-harness.sh start          relay + B creates a vault (ZAFE_THRESHOLD of
#                                         ZAFE_MEMBERS, default 2 of 3); prints the invite
#   scripts/app-harness.sh join-all       the other CLI members (C, D, ...) join; alias join-c
#   scripts/app-harness.sh seal           B seals (after the app joined); shows the members
#   scripts/app-harness.sh keygen         CLI members run keygen in the background (app joins in)
#   scripts/app-harness.sh each ARGS      run the zafe CLI as every CLI member in turn
#   scripts/app-harness.sh fund           regtest up (ths), faucet notes to the vault
#   scripts/app-harness.sh mine N         mine N blocks
#   scripts/app-harness.sh resume         after a restart: relay, adb reverse, and a fresh
#                                         ths chain, funded again (ths never keeps a chain)
#   scripts/app-harness.sh cli WHO ARGS   run the zafe CLI as one member (sync, approve, respond, ...)
#   scripts/app-harness.sh stop           stop everything and delete the state
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
# Outside /tmp so a machine restart keeps the relay DB and the CLI members.
WORK=${ZAFE_HARNESS_DIR:-$HOME/.cache/zafe-harness}
export ZAFE_RELAY=http://127.0.0.1:8787 ZAFE_LIGHTWALLETD=http://127.0.0.1:9067
ZAFE="$ROOT/target/debug/zafe"
member() { local who=$1; shift; "$ZAFE" --home "$WORK/$who" "$@"; }
# CLI members: B plus the others that join (ZAFE_MEMBERS - 1 in total; the app is the last seat).
others() { cat "$WORK/others"; }
# ths environment `zafe-regtest` on its default ports (lightwalletd 9067).
mine() { "${THS:-ths}" --name zafe-regtest mine "$1" >/dev/null; }
fund() {
  local addr
  addr=$(member B vault show | sed -n 's/^address //p')
  "$ROOT/infra/regtest/up.sh"
  "$ROOT/infra/regtest/fund.sh" "$addr" "${ZAFE_FUND_NOTES:-6}"
}

relay_up() {
  ZAFE_RELAY_LISTEN=127.0.0.1:8787 ZAFE_RELAY_DB="$WORK/relay.sqlite" \
    nohup "$ROOT/target/debug/zafe-relay" >> "$WORK/relay.log" 2>&1 &
  echo $! > "$WORK/relay.pid"
  for _ in $(seq 50); do (exec 3<>/dev/tcp/127.0.0.1/8787) 2>/dev/null && break; sleep 0.2; done
  adb reverse tcp:8787 tcp:8787
  adb reverse tcp:9067 tcp:9067
}

case "${1:-}" in
  start)
    mkdir -p "$WORK"
    cargo build -q -p zafe-cli -p zafe-relay
    relay_up
    T=${ZAFE_THRESHOLD:-2} N=${ZAFE_MEMBERS:-3}
    echo C D E F G H I J K L M N O | tr ' ' '\n' | head -n $((N - 2)) > "$WORK/others"
    member B init >/dev/null
    for who in $(others); do member "$who" init >/dev/null; done
    member B vault create --name "${ZAFE_VAULT_NAME:-Grants}" --threshold "$T" --members "$N" | tee "$WORK/invite"
    ;;
  join-c|join-all) for who in $(others); do member "$who" vault join "$(cat "$WORK/invite")"; done ;;
  seal) member B vault seal; member B vault members ;;
  keygen)
    # The safety check needs every member at once (the app: keep its setup screen open).
    pids=()
    for who in B $(others); do
      member "$who" vault members --wait > "$WORK/m$who" 2>&1 & pids+=($!)
    done
    for pid in "${pids[@]}"; do wait "$pid" || { cat "$WORK/m"*; exit 1; }; done
    SN=$(sed -n 's/^safety words: //p' "$WORK/mB")
    echo "safety words: $SN"
    nohup "$ZAFE" --home "$WORK/B" vault keygen --safety-number "$SN" --birthday 2 > "$WORK/kB" 2>&1 &
    for who in $(others); do
      nohup "$ZAFE" --home "$WORK/$who" vault keygen --safety-number "$SN" > "$WORK/k$who" 2>&1 &
    done
    ;;
  fund) fund ;;
  resume)
    # After a machine or emulator restart: same relay DB and members. ths always starts a
    # new chain, so the vault is funded again and the CLI members' wallets start over. The
    # app's wallet still holds the old chain: if it doesn't recover, remove and restore the
    # vault on the device (untested).
    cargo build -q -p zafe-cli -p zafe-relay
    relay_up
    if [[ -f "$WORK/B/vault.bin" ]]; then
      fund
      for who in B $(others); do rm -f "$WORK/$who"/wallet.sqlite*; done
    fi
    echo "resumed (state in $WORK)"
    ;;
  mine) mine "${2:-1}" ;;
  cli) who=$2; shift 2; member "$who" "$@" ;;
  each) shift; for who in B $(others); do echo "== $who"; member "$who" "$@"; done ;;
  stop)
    [[ -f "$WORK/relay.pid" ]] && kill "$(cat "$WORK/relay.pid")" 2>/dev/null || true
    "$ROOT/infra/regtest/down.sh" || true
    rm -rf "$WORK"
    ;;
  *) sed -n '2,15p' "$0"; exit 1 ;;
esac

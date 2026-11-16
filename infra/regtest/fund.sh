#!/usr/bin/env bash
set -euo pipefail
ADDRESS=${1:?usage: fund.sh <unified-address> [notes]}
NOTES=${2:-4}
THS=${THS:-ths}
NAME=${ZAFE_REGTEST_NAME:-zafe-regtest}

for _ in $(seq "$NOTES"); do
  "$THS" --name "$NAME" faucet "$ADDRESS" --amount 5 >/dev/null
done
"$THS" --name "$NAME" mine 12 >/dev/null
echo "funded $ADDRESS with $NOTES x 5 ZEC"

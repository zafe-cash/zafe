#!/usr/bin/env bash
set -u
url=$1; sel=$2; vw=$3; vh=$4; frames=$5; out=$6; tag=${7:-f}; settle=${8:-1800}
mkdir -p "$out"
S="agent-browser --session frames-$tag"
$S set media light >/dev/null 2>&1
$S set viewport "$vw" "$vh" >/dev/null
$S open about:blank >/dev/null
$S open "$url" >/dev/null
$S wait 5000 >/dev/null

box() { $S get box "$1" 2>/dev/null | awk -v k="$2" '$1==k":"{print int($2)}'; }
pos=0
go() {
  local t=$1 d
  while [ "$pos" -lt "$t" ]; do
    d=$(( t - pos )); [ $d -gt 1500 ] && d=1500
    $S scroll down $d >/dev/null; pos=$(( pos + d )); $S wait 150 >/dev/null
  done
}

if [ "$sel" = page ]; then
  start=0
  range=$(( $(box body height) - vh ))
else
  y=$(box "$sel" y); h=$(box "$sel" height)
  [ -z "$y" ] && { echo "no element for $sel"; exit 1; }
  start=$(( y - vh )); [ $start -lt 0 ] && start=0
  range=$(( y + h - vh - start ))
fi
echo "range $start..$(( start + range ))"
for n in $(seq 0 "$frames"); do
  go $(( start + range * n / frames ))
  $S wait "$settle" >/dev/null
  $S screenshot "$out/${tag}_$n.png" >/dev/null
done
$S errors 2>&1 | tail -3
$S console 2>&1 | grep -iv "data-annot" | grep -iE "error|warn" | tail -5
echo "wrote $(( frames + 1 )) frames to $out"

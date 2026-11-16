#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="$here/public/assets/stills"
raw="$(mktemp -d)"
port=8797
mkdir -p "$out"
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$here/dist" >/dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null; rm -rf "$raw"' EXIT
S="agent-browser --session stills"

box() { $S get box "$1" 2>/dev/null | awk -v k="$2" '$1==k":"{print int($2)}'; }

shoot() {
  local query=$1 w=$2 h=$3 scale=$4; shift 4
  $S set viewport "$w" "$h" "$scale" >/dev/null
  $S open about:blank >/dev/null
  $S open "http://127.0.0.1:$port/showcase.html?$query" >/dev/null
  $S wait 9000 >/dev/null
  local pos=0
  while [ $# -ge 3 ]; do
    local name=$1 sel=$2 frac=$3; shift 3
    local y hh start range t d
    y=$(box "$sel" y); hh=$(box "$sel" height)
    y=$(( y + pos ))
    start=$(( y - h )); [ $start -lt 0 ] && start=0
    range=$(( y + hh - h - start ))
    t=$(python3 -c "print(int($start + $range * $frac))")
    while [ "$pos" -lt "$t" ]; do
      d=$(( t - pos )); [ $d -gt 1500 ] && d=1500
      $S scroll down $d >/dev/null; pos=$(( pos + d )); $S wait 150 >/dev/null
    done
    $S wait 14000 >/dev/null
    $S screenshot "$raw/$name.png" >/dev/null
    echo "shot $name at $t"
  done
}

shoot still 1440 900 1 \
  hero_wide .hero 0 \
  beat_1 .story 0.06 \
  beat_2 .story 0.28 \
  beat_3 .story 0.62 \
  beat_4 .story 0.74 \
  beat_5 .story 0.93 \
  above .cta-world 1
shoot still 390 844 2 hero_tall .hero 0
$S set viewport 1200 630 1 >/dev/null
$S open "http://127.0.0.1:$port/index.html" >/dev/null
$S wait 4500 >/dev/null
$S screenshot "$raw/og.png" >/dev/null
echo "shot og"

"${PYTHON:-python3}" - "$raw" "$out" "$here/public/assets/og-mainnet.png" <<'PY'
import sys
from PIL import Image
raw, out, og = sys.argv[1:4]
def save(name, crop=None, width=None, q=80):
    im = Image.open(f'{raw}/{name}.png').convert('RGB')
    if crop:  # fractions of the frame: left, top, right, bottom
        w, h = im.size
        im = im.crop((int(crop[0] * w), int(crop[1] * h), int(crop[2] * w), int(crop[3] * h)))
    if width and im.width > width:
        im = im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)
    im.save(f'{out}/{name}.webp', 'WEBP', quality=q, method=6)
    print(name, im.size)
save('hero_wide')
save('hero_tall')
for i in range(1, 6):
    save(f'beat_{i}', width=1200)
save('above', width=1440)
Image.open(f'{raw}/og.png').convert('RGB').save(og, optimize=True)
print('og', Image.open(og).size)
PY

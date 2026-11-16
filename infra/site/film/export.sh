#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/../public/assets/motion"
browser="${REMOTION_BROWSER:-$(ls -d ~/.agent-browser/browsers/chrome-*/chrome 2>/dev/null | head -1)}"
args=(--log=error --concurrency=4)
[[ -n "$browser" ]] && args+=(--browser-executable="$browser")
mkdir -p "$out" "$here/out"
cd "$here"
render() {
  npx remotion render "$1" "out/$1.mp4" --codec=h264 --crf=18 "${args[@]}"
  ffmpeg -loglevel error -y -i "out/$1.mp4" -c:v libx264 -preset veryslow -tune animation -crf 24 \
    -pix_fmt yuv420p -movflags +faststart -an "$out/$1.mp4"
  npx remotion still "$1" "out/$1-poster.png" --frame=0 "${args[@]}"
  npx remotion still "$1" "out/$1-still.png" --frame="$2" "${args[@]}"
  ffmpeg -loglevel error -y -i "out/$1-poster.png" -c:v libwebp -quality 82 "$out/$1.webp"
  ffmpeg -loglevel error -y -i "out/$1-still.png" -c:v libwebp -quality 82 "$out/$1-still.webp"
}
for c in sign:80 sent:70 setup:90 live:40; do
  [[ $# -gt 0 && ! " $* " =~ " ${c%%:*} " ]] && continue
  render "${c%%:*}" "${c##*:}"
  render "${c%%:*}-tall" "${c##*:}"
done
ui="$here/../public/assets/ui"
python3 - "$ui" <<'PY'
import sys
ui = sys.argv[1]
heights = {"sign": 1602, "sent": 1008, "setup": 1188, "live": 1293}
for name, h in heights.items():
    for tall, (W, H) in ((False, (1600, 900)), (True, (720, 1280))):
        w = W * 0.58 if tall else H * 0.78 * 1080 / 1602
        ch = h * w / 1080
        x, y = (W - w) / 2, (H * 0.4 - ch / 2) if tall else (H - ch) / 2
        r = w * 0.078
        hole = (f"M{x + r:.1f} {y:.1f}H{x + w - r:.1f}A{r:.1f} {r:.1f} 0 0 1 {x + w:.1f} {y + r:.1f}"
                f"V{y + ch - r:.1f}A{r:.1f} {r:.1f} 0 0 1 {x + w - r:.1f} {y + ch:.1f}H{x + r:.1f}"
                f"A{r:.1f} {r:.1f} 0 0 1 {x:.1f} {y + ch - r:.1f}V{y + r:.1f}A{r:.1f} {r:.1f} 0 0 1 {x + r:.1f} {y:.1f}Z")
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" preserveAspectRatio="none">'
               f'<path fill-rule="evenodd" d="M0 0H{W}V{H}H0Z{hole}"/></svg>\n')
        open(f"{ui}/mask-{name}{'-tall' if tall else ''}.svg", "w").write(svg)
PY
[[ -f "$ui/grain.png" ]] || convert -size 256x256 xc:gray50 +noise Gaussian -colorspace Gray -blur 0x0.4 \
  -level 30%,70% -alpha copy -fill "#3A3020" -colorize 100 "$ui/grain.png"
ls -l "$out" | awk '{print $5, $9}'

#!/usr/bin/env python3
"""Checks that every theme colour token means the same thing in dark and light mode.

Each file in app/lib/src/core/theme/colors/ defines `static const dark = X(...)` and
`static const light = X(...)`. For every token both values are resolved to a colour
(from app/lib/src/core/theme/primitives.dart, or a literal `Color(0x...)`) and the pair
is flagged when:

  * ramp differs: both sides come from primitive ramps, but different ones (the sync
    glow was gold in light mode and the neutral grey in dark, 2026-10-27);
  * a foreground token (text, icon, label, sync, ticker) reads as a colour in one theme
    and near-grey in the other (OKLCH chroma under 0.06 against 0.075 or more): the sync
    text was gold in light mode and cream in dark, both from the gold ramp;
  * literal colours: one is a colour and the other grey, or the hues differ by more
    than 45 degrees;
  * a token is set in one theme only.

A deliberate difference is allowed with a comment on either line:
`// theme-pair: <why>`.

Usage: scripts/check_theme_pairs.py   (exit 1 on findings)
"""
import math
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
THEME = ROOT / "app/lib/src/core/theme"
PRIMITIVES = THEME / "primitives.dart"
COLORS = THEME / "colors"

# OKLCH chroma: under GREY_CHROMA a colour is grey; a foreground token under
# FAINT_CHROMA reads grey next to one at COLOURED_CHROMA or more. Near-transparent and
# near-black colours carry little hue, so they are only compared when visible.
GREY_CHROMA = 0.03
FAINT_CHROMA = 0.06
COLOURED_CHROMA = 0.075
MIN_ALPHA = 0x30
MIN_LIGHTNESS = 0.15
MAX_HUE_DIFF = 45
FOREGROUND_CLASSES = {"AppTextColors", "AppIconColors", "AppSyncColors"}
FOREGROUND_FIELD = re.compile(r"(text|label|icon|ticker|glow|fg)", re.I)


def parse_primitives():
    """{'GoldPrimitives.p500Dark': 0xAARRGGBB}"""
    values = {}
    family = None
    for line in PRIMITIVES.read_text().splitlines():
        m = re.match(r"\s*abstract final class (\w+)", line)
        if m:
            family = m.group(1)
            continue
        m = re.match(r"\s*static const (\w+) = Color\(0x([0-9A-Fa-f]{8})\);", line)
        if m and family:
            values[f"{family}.{m.group(1)}"] = int(m.group(2), 16)
    return values


def parse_sets(text):
    """[(class, {'dark': {field: (expr, line, allowed)}, 'light': {...}})]"""
    sets = {}
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        m = re.match(r"\s*static const (dark|light) = (\w+)\(", lines[i])
        if not m:
            i += 1
            continue
        theme, cls = m.group(1), m.group(2)
        fields = {}
        depth = lines[i].count("(") - lines[i].count(")")
        i += 1
        while i < len(lines) and depth > 0:
            line = lines[i]
            fm = re.match(r"\s*(\w+):\s*(.+?),?\s*(//.*)?$", line)
            if fm and depth == 1:
                expr = fm.group(2).rstrip(",").strip()
                allowed = "theme-pair:" in line or "theme-pair:" in lines[i - 1]
                fields[fm.group(1)] = (expr, i + 1, allowed)
            depth += line.count("(") - line.count(")")
            i += 1
        sets.setdefault(cls, {})[theme] = fields
    return sets


def resolve(expr, prims):
    """(argb, ramp or None) for a primitive or a literal; None for anything else."""
    m = re.fullmatch(r"(?:const\s+)?Color\(0x([0-9A-Fa-f]{8})\)", expr)
    if m:
        return int(m.group(1), 16), None
    m = re.fullmatch(r"(\w*Primitives)\.(\w+)", expr)
    if m and expr in prims:
        return prims[expr], m.group(1)
    return None


def oklch(argb):
    """(alpha 0-255, L, C, hue degrees)"""
    a = (argb >> 24) & 0xFF
    rgb = [((argb >> sh) & 0xFF) / 255 for sh in (16, 8, 0)]
    r, g, b = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
    l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s
    ca = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s
    cb = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s
    return a, lightness, math.hypot(ca, cb), math.degrees(math.atan2(cb, ca)) % 360


def visible(argb):
    a, lightness, _, _ = oklch(argb)
    return a >= MIN_ALPHA and lightness >= MIN_LIGHTNESS


def main():
    prims = parse_primitives()
    findings = []
    for path in sorted(COLORS.glob("*.dart")):
        rel = path.relative_to(ROOT)
        for cls, themes in parse_sets(path.read_text()).items():
            dark, light = themes.get("dark", {}), themes.get("light", {})
            for field in sorted(set(dark) | set(light)):
                if field not in dark or field not in light:
                    where = dark.get(field) or light.get(field)
                    findings.append(f"{rel}:{where[1]} {cls}.{field}: set in one theme only")
                    continue
                (de, dl, da), (le, ll, la) = dark[field], light[field]
                if da or la:
                    continue
                d, l = resolve(de, prims), resolve(le, prims)
                if d is None or l is None:
                    continue  # another token or an expression: checked where it's defined
                (dc, dramp), (lc, lramp) = d, l
                tag = f"{rel}:{dl} {cls}.{field}: dark {de} #{dc:08X}, light {le} #{lc:08X}"
                if dramp and lramp and dramp != lramp:
                    findings.append(f"{tag}: different ramps")
                    continue
                if not (visible(dc) and visible(lc)):
                    continue
                _, _, dch, dh = oklch(dc)
                _, _, lch, lh = oklch(lc)
                neutral = dramp == "Primitives" and lramp == "Primitives"
                foreground = cls in FOREGROUND_CLASSES or FOREGROUND_FIELD.search(field)
                if foreground and not neutral and min(dch, lch) < FAINT_CHROMA <= max(
                    dch, lch
                ) and max(dch, lch) >= COLOURED_CHROMA:
                    faint = "dark" if dch < lch else "light"
                    findings.append(
                        f"{tag}: reads grey in {faint} (chroma {dch:.3f} vs {lch:.3f})"
                    )
                    continue
                if dramp and lramp:
                    continue  # one ramp: same hue by construction
                if (dch < GREY_CHROMA) != (lch < GREY_CHROMA):
                    findings.append(f"{tag}: grey in one theme, coloured in the other")
                    continue
                if dch >= GREY_CHROMA:
                    diff = min(abs(dh - lh), 360 - abs(dh - lh))
                    if diff > MAX_HUE_DIFF:
                        findings.append(f"{tag}: hue differs by {diff:.0f} degrees")
    for f in findings:
        print(f)
    if findings:
        print(f"\n{len(findings)} theme pair(s) differ. Fix them, or mark a deliberate one "
              "with `// theme-pair: <why>`.", file=sys.stderr)
        return 1
    print("theme pairs match")
    return 0


if __name__ == "__main__":
    sys.exit(main())

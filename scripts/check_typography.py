#!/usr/bin/env python3
"""Keeps the app's type on its scale (app/lib/src/core/theme/app_typography.dart).

Flags, in app/lib (generated bridge code excluded):
  * a `fontSize:` or `fontFamily:` written outside the theme: use an AppTypography role
    (hero, title, heading, amount, body..., mono) and copyWith colour or weight only;
  * a text size under 14 in the mobile scale (below it reading slows; Legge & Bigelow
    2011). The desktop set is left alone;
  * the `lnum` / `case` font features: Space Grotesk's bundled files have neither, so
    they do nothing (amounts use kTabularFigures);
  * a font family that isn't bundled in app/pubspec.yaml, in code or comments, so a
    family left over from an old design file (Geist, Young Serif, Libre Caslon...) can't
    linger.
A deliberate exception carries `// type: <why>` on its line or the line before.

Usage: scripts/check_typography.py   (exit 1 on findings)
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
APP = ROOT / "app"
THEME = APP / "lib/src/core/theme/app_typography.dart"
MIN_SIZE = 14


def bundled_families():
    pubspec = (APP / "pubspec.yaml").read_text()
    return set(re.findall(r"^\s*- family:\s*(.+?)\s*$", pubspec, re.M))


def allowed(lines, i):
    return "type:" in lines[i] or (i > 0 and "// type:" in lines[i - 1])


def main():
    families = bundled_families()
    other_fonts = {"Geist", "Young Serif", "Libre Caslon", "Inter", "Roboto", "SF Pro"} - families
    findings = []
    files = [p for p in (APP / "lib").rglob("*.dart") if "/rust/" not in p.as_posix()]
    for path in sorted(files):
        rel = path.relative_to(ROOT)
        lines = path.read_text().splitlines()
        in_theme = path == THEME
        for i, line in enumerate(lines):
            if allowed(lines, i):
                continue
            where = f"{rel}:{i + 1}"
            if not in_theme and re.search(r"\bfontSize\s*:", line):
                findings.append(f"{where}: fontSize outside the theme; use an AppTypography role")
            if not in_theme and re.search(r"\bfontFamily\s*:", line):
                findings.append(f"{where}: fontFamily outside the theme; use an AppTypography role")
            if re.search(r"liningFigures\(\)|enable\('case'\)|'lnum'", line):
                findings.append(f"{where}: lnum/case do nothing in the bundled fonts")
            for name in other_fonts:
                if re.search(rf"\b{re.escape(name)}\b", line):
                    findings.append(f"{where}: '{name}' isn't a bundled font (stale reference?)")
    # The mobile scale: no size under MIN_SIZE (the shared roles and the mobile class).
    text = THEME.read_text()
    theme_lines = text.splitlines()
    desktop = re.search(r"abstract final class AppTypographyDesktop \{.*?\n\}", text, re.S)
    for m in re.finditer(r"fontSize:\s*([0-9.]+)", text):
        if desktop and desktop.start() <= m.start() < desktop.end():
            continue
        line = text.count("\n", 0, m.start()) + 1
        if allowed(theme_lines, line - 1):
            continue
        if float(m.group(1)) < MIN_SIZE:
            findings.append(f"{THEME.relative_to(ROOT)}:{line}: size {m.group(1)} is under {MIN_SIZE}")
    for f in findings:
        print(f)
    if findings:
        print(f"\n{len(findings)} place(s) off the type scale.", file=sys.stderr)
        return 1
    print("type is on the scale")
    return 0


if __name__ == "__main__":
    sys.exit(main())

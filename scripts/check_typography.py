#!/usr/bin/env python3
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
APP = ROOT / "app"
THEME = APP / "lib/src/core/theme/app_typography.dart"
THEME_FILES = {THEME, APP / "lib/src/core/theme/material_theme.dart"}
MIN_SIZE = 14


def bundled_families():
    pubspec = (APP / "pubspec.yaml").read_text()
    return set(re.findall(r"^\s*- family:\s*(.+?)\s*$", pubspec, re.M))


def main():
    families = bundled_families()
    other_fonts = {"Geist", "Young Serif", "Libre Caslon", "Inter", "Roboto", "SF Pro"} - families
    findings = []
    files = [p for p in (APP / "lib").rglob("*.dart") if "/rust/" not in p.as_posix()]
    for path in sorted(files):
        rel = path.relative_to(ROOT)
        lines = path.read_text().splitlines()
        in_theme = path in THEME_FILES
        for i, line in enumerate(lines):
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
    text = THEME.read_text()
    for m in re.finditer(r"fontSize:\s*([0-9.]+)", text):
        line = text.count("\n", 0, m.start()) + 1
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

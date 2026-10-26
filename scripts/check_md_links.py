#!/usr/bin/env python3
"""Checks that every relative link in the repo's Markdown resolves.

For each tracked *.md file (git ls-files), every [text](target) link that isn't a URL,
mailto or pure #anchor must point to a file or directory that exists; a `#heading` on a
Markdown target must match one of its headings (GitHub's slug rules: lower case, spaces
to hyphens, punctuation dropped). Code spans and fenced blocks are skipped.

Usage: scripts/check_md_links.py   (exit 1 on broken links)
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")


def slug(heading):
    text = re.sub(r"[`*_~]", "", heading.strip().lower())
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
    return text.replace(" ", "-")


def anchors(path, cache={}):
    if path not in cache:
        found = set()
        counts = {}
        in_fence = False
        for line in path.read_text(errors="ignore").splitlines():
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
            if in_fence:
                continue
            m = re.match(r"#{1,6}\s+(.*)", line)
            if m:
                s = slug(m.group(1))
                n = counts.get(s, 0)
                counts[s] = n + 1
                found.add(s if n == 0 else f"{s}-{n}")
        cache[path] = found
    return cache[path]


def main():
    files = subprocess.run(
        ["git", "ls-files", "*.md"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    broken = []
    for name in files:
        path = ROOT / name
        if "node_modules" in name or not path.exists():
            continue
        in_fence = False
        for no, line in enumerate(path.read_text(errors="ignore").splitlines(), 1):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            line = re.sub(r"`[^`]*`", "", line)
            for m in LINK.finditer(line):
                target = m.group(1)
                if re.match(r"^[a-z]+:", target) or target.startswith("#") or target.startswith("<"):
                    continue
                file_part, _, anchor = target.partition("#")
                dest = (path.parent / file_part).resolve() if file_part else path
                if not dest.exists():
                    broken.append(f"{name}:{no}: {target} (no such file)")
                elif anchor and dest.suffix == ".md" and anchor.lower() not in anchors(dest):
                    broken.append(f"{name}:{no}: {target} (no heading #{anchor})")
    for b in broken:
        print(b)
    if broken:
        print(f"\n{len(broken)} broken link(s).", file=sys.stderr)
        return 1
    print("markdown links resolve")
    return 0


if __name__ == "__main__":
    sys.exit(main())

import re
import sys

from check_no_comments import ROOT, exempt, tracked

SILENCERS = {
    ".rs": re.compile(r"#!?\[\s*(?:allow|expect)\s*\(([^)]*)\)"),
    ".kt": re.compile(r"@(?:file:)?Suppress\s*\(([^)]*)\)"),
    ".kts": re.compile(r"@(?:file:)?Suppress\s*\(([^)]*)\)"),
}

APPROVED = {
    ("crates/zafe-core/src/signing.rs", "deprecated"),
}


def main() -> int:
    found, used = [], set()
    for path in tracked():
        pattern = next((p for suffix, p in SILENCERS.items() if path.endswith(suffix)), None)
        full = ROOT / path
        if pattern is None or not full.is_file():
            continue
        text = full.read_text(encoding="utf-8")
        if exempt(path, text):
            continue
        for m in pattern.finditer(text):
            lints = m.group(1).strip()
            if (path, lints) in APPROVED:
                used.add((path, lints))
                continue
            line = text.count("\n", 0, m.start()) + 1
            found.append(f"{path}:{line}: {m.group(0)}")
    stale = sorted(APPROVED - used)
    for f in found:
        print(f)
    for path, lints in stale:
        print(f"{path}: approved silencing of {lints} is gone; remove it from APPROVED")
    if found or stale:
        print(f"\n{len(found)} silenced lints. Fix the code, or change the rule centrally.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

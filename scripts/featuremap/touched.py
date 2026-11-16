import fnmatch
import re
import subprocess
from dataclasses import dataclass, field

from . import catalog
from .check import ROOT, features, owners, symbol_ranges
from .model import Files, Flutter, Rust, Symbols


@dataclass
class Touched:
    features: set[str] = field(default_factory=set)
    plumbing: set[str] = field(default_factory=set)
    unowned: set[str] = field(default_factory=set)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def changed_lines(base: str, head: str) -> dict[str, set[int]]:
    lines: dict[str, set[int]] = {}
    current = None
    for line in git("diff", "-U0", "--no-renames", base, head).splitlines():
        if line.startswith("+++ "):
            current = None if line.endswith("/dev/null") else line[6:]
        elif line.startswith("--- ") and current is None:
            continue
        elif line.startswith("@@") and current:
            m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", line)
            start, count = int(m.group(1)), int(m.group(2) or "1")
            lines.setdefault(current, set()).update(range(start, start + max(count, 1)))
    return lines


def file_owners(path: str) -> list[str]:
    found = []
    for owner, owns in owners():
        for own in owns:
            if isinstance(own, Files) and any(fnmatch.fnmatch(path, p) for p in own.patterns):
                found.append(owner)
    return found


def split_owners(path: str) -> list[tuple[str, tuple[str, ...]]]:
    found = []
    for owner, owns in owners():
        for own in owns:
            if isinstance(own, Symbols) and own.file == path and own.anchors:
                found.append((owner, own.anchors))
    return found


def rest_owner(path: str) -> str | None:
    for owner, owns in owners():
        for own in owns:
            if isinstance(own, Symbols) and own.file == path and not own.anchors:
                return owner
    return None


def proof_owners(path: str) -> list[str]:
    found = []
    for f in features():
        for p in f.proofs:
            if isinstance(p, Flutter) and path == f"app/{p.file}":
                found.append(f.id)
            if isinstance(p, Rust) and p.target and path == f"{catalog.CRATES[p.crate]}/tests/{p.target}.rs":
                found.append(f.id)
    return found


def add(result: Touched, owner: str):
    if owner.startswith("plumbing:"):
        result.plumbing.add(owner[len("plumbing:"):])
    else:
        result.features.add(owner)


def touched(base: str, head: str) -> Touched:
    result = Touched()
    names = [p for p in git("diff", "--name-only", "--no-renames", base, head).splitlines() if p]
    lines = changed_lines(base, head)
    for path in names:
        hit = False
        for owner in file_owners(path) + proof_owners(path):
            add(result, owner)
            hit = True
        split = split_owners(path)
        if split:
            hit = True
            changed = lines.get(path, set())
            claimed = set()
            exists = (ROOT / path).is_file()
            for owner, anchors in split:
                if not exists:
                    add(result, owner)
                    continue
                for spans in symbol_ranges(path, anchors).values():
                    for start, end in spans:
                        inside = {n for n in changed if start <= n <= end}
                        if inside:
                            add(result, owner)
                            claimed |= inside
            rest = rest_owner(path)
            if rest and (changed - claimed or not exists):
                add(result, rest)
        if not hit:
            result.unowned.add(path)
    return result

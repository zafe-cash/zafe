import ast
import fnmatch
import re
import subprocess
from pathlib import Path

from . import catalog
from .model import App, Cli, Files, Flutter, Label, RelayRoute, Rust, Script, Symbols, NEEDS

ROOT = Path(__file__).resolve().parents[2]
APP_LIB = ROOT / "app/lib"
ROUTER = ROOT / "app/lib/src/app.dart"
CLI_MAIN = ROOT / "crates/zafe-cli/src/main.rs"
RELAY_LIB = ROOT / "crates/zafe-relay/src/lib.rs"
FLOWS = Path(__file__).resolve().parent / "flows.py"
TEST_ATTR = re.compile(r"^\s*#\[(?:tokio::)?test(?:\([^)]*\))?\]")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    return [p for p in out.splitlines() if (ROOT / p).is_file()]


def features():
    for area in catalog.AREAS:
        yield from area.features


def owners():
    for f in features():
        yield f.id, f.owns
    for p in catalog.PLUMBING:
        yield f"plumbing:{p.name}", p.owns


def all_proofs():
    for f in features():
        yield f.id, f.proofs
    for p in catalog.PLUMBING:
        yield f"plumbing:{p.name}", p.proofs


def anchor_regex(file: str, name: str) -> re.Pattern:
    if "::" in name:
        return re.compile(rf"^\s*{re.escape(name)}\b")
    return re.compile(rf"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:const\s+)?fn\s+{re.escape(name)}\b")


def block_end(lines: list[str], start: int) -> int:
    indent = len(lines[start]) - len(lines[start].lstrip())
    for i in range(start + 1, len(lines)):
        line = lines[i]
        if not line.strip():
            continue
        current = len(line) - len(line.lstrip())
        if current < indent:
            return i - 1
        if current == indent:
            return i if line.strip().startswith(("}", ")", "]")) else i - 1
    return len(lines) - 1


def symbol_ranges(file: str, names: tuple[str, ...]) -> dict[str, list[tuple[int, int]]]:
    lines = (ROOT / file).read_text().splitlines()
    ranges: dict[str, list[tuple[int, int]]] = {}
    for name in names:
        pattern = anchor_regex(file, name)
        for i, line in enumerate(lines):
            if pattern.search(line):
                ranges.setdefault(name, []).append((i + 1, block_end(lines, i) + 1))
    return ranges


def routes() -> set[str]:
    lines = ROUTER.read_text().splitlines()
    found, parents = set(), []
    for line in lines:
        indent = len(line) - len(line.lstrip())
        for m in re.finditer(r"path: '([^']+)'", line):
            path = m.group(1)
            while parents and parents[-1][0] >= indent:
                parents.pop()
            if path.startswith("/"):
                found.add(path)
                parents.append((indent, path))
            elif parents:
                found.add(parents[-1][1] + "/" + path)
        for m in re.finditer(r"\('(/[^']+)', \w+Screen\(\)\)", line):
            found.add(m.group(1))
    return found


def cli_commands() -> set[str]:
    text = CLI_MAIN.read_text()
    found = set()
    enums = {"Command": "", "VaultCmd": "vault ", "SeatCmd": "seat "}
    for enum, prefix in enums.items():
        m = re.search(rf"enum {enum} \{{(.*?)\n\}}", text, re.S)
        if not m:
            continue
        for v in re.finditer(r"^    ([A-Z]\w*)\b", m.group(1), re.M):
            name = v.group(1)
            if name in ("Vault", "Seat"):
                continue
            kebab = re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()
            found.add(prefix + kebab)
    return found


def relay_routes() -> set[str]:
    return set(re.findall(r'\.route\(\s*"([^"]+)"', RELAY_LIB.read_text()))


def app_strings() -> str:
    return "\n".join(p.read_text() for p in APP_LIB.rglob("*.dart") if "/src/rust/" not in p.as_posix())


def label_present(text: str, source: str) -> bool:
    variants = {text, text.replace("'", "\\'"), text.replace("'", "’")}
    return any(f"'{v}'" in source or f'"{v}"' in source or f"'{v}" in source or f"{v}'" in source for v in variants)


def rust_tests() -> dict[tuple[str, str | None], set[str]]:
    found: dict[tuple[str, str | None], set[str]] = {}
    for crate, base in catalog.CRATES.items():
        for path in sorted((ROOT / base).rglob("*.rs")):
            rel = path.relative_to(ROOT / base).as_posix()
            if rel.startswith("target/") or "/frb_generated" in rel:
                continue
            if rel.startswith("tests/") and rel.count("/") == 1:
                key = (crate, path.stem)
            elif rel.startswith("src/"):
                key = (crate, None)
            else:
                continue
            lines = path.read_text().splitlines()
            for i, line in enumerate(lines):
                if TEST_ATTR.match(line):
                    for j in range(i + 1, min(i + 6, len(lines))):
                        m = FN.match(lines[j])
                        if m:
                            found.setdefault(key, set()).add(m.group(1))
                            break
    return found


def ignored_tests() -> set[tuple[str, str | None, str]]:
    found = set()
    for crate, base in catalog.CRATES.items():
        for path in sorted((ROOT / base).rglob("*.rs")):
            rel = path.relative_to(ROOT / base).as_posix()
            if not (rel.startswith("tests/") or rel.startswith("src/")):
                continue
            key = path.stem if rel.startswith("tests/") and rel.count("/") == 1 else None
            lines = path.read_text().splitlines()
            for i, line in enumerate(lines):
                if re.match(r"^\s*#\[ignore", line):
                    for j in range(i + 1, min(i + 6, len(lines))):
                        m = FN.match(lines[j])
                        if m:
                            found.add((crate, key, m.group(1)))
                            break
    return found


def flutter_test_files() -> set[str]:
    return {p.relative_to(ROOT / "app").as_posix()
            for d in ("test", "tool") for p in (ROOT / "app" / d).rglob("*_test.dart")}


def flow_module():
    tree = ast.parse(FLOWS.read_text())
    defined = {n.name for n in tree.body if isinstance(n, ast.FunctionDef) and not n.name.startswith("_")}
    visits: dict[str, set[str]] = {}
    for fn in tree.body:
        if not isinstance(fn, ast.FunctionDef):
            continue
        for node in ast.walk(fn):
            if (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "on"
                    and node.args and isinstance(node.args[0], ast.Constant)):
                visits.setdefault(fn.name, set()).add(node.args[0].value)
    return defined, visits


def dart_reachable() -> set[str]:
    seen, todo = set(), [APP_LIB / "main.dart"]
    while todo:
        path = todo.pop().resolve()
        if path in seen or not path.exists():
            continue
        seen.add(path)
        for statement in re.finditer(r"^(?:import|export|part)\b[^;]*;", path.read_text(), re.M | re.S):
            for target in re.findall(r"'([^']+\.dart)'", statement.group(0)):
                if target.startswith("package:zafe/"):
                    todo.append(APP_LIB / target[len("package:zafe/"):])
                elif not target.startswith(("package:", "dart:")):
                    todo.append(path.parent / target)
    return {p.relative_to(ROOT).as_posix() for p in seen}


def check() -> list[str]:
    errors: list[str] = []
    files = tracked()
    in_roots = [f for f in files if f.startswith(tuple(r + "/" for r in catalog.ROOTS))]

    ids = [f.id for f in features()]
    for dup in {i for i in ids if ids.count(i) > 1}:
        errors.append(f"feature id {dup} is used twice")

    claimed_files: dict[str, list[str]] = {}
    split_files: dict[str, list[str]] = {}
    rest_files: set[str] = set()
    for owner, owns in owners():
        for own in owns:
            if isinstance(own, Files):
                for pattern in own.patterns:
                    matched = [f for f in files if fnmatch.fnmatch(f, pattern)]
                    if not matched:
                        errors.append(f"{owner}: {pattern} matches no file")
                    for f in matched:
                        claimed_files.setdefault(f, []).append(owner)
            elif isinstance(own, Symbols):
                if not (ROOT / own.file).is_file():
                    errors.append(f"{owner}: {own.file} doesn't exist")
                    continue
                if not own.anchors:
                    rest_files.add(own.file)
                    continue
                split_files.setdefault(own.file, []).append(owner)
                ranges = symbol_ranges(own.file, own.anchors)
                for name in own.anchors:
                    if name not in ranges:
                        errors.append(f"{owner}: {own.file} has no `{name}`")

    for f in in_roots:
        if f not in claimed_files and f not in split_files and f not in rest_files:
            errors.append(f"{f}: no feature owns it (add it to a feature in scripts/featuremap/catalog.py)")
    for f in split_files:
        if f not in rest_files:
            errors.append(f"{f}: split by symbols but no plumbing owns the rest")

    app_routes = routes()
    source = app_strings()
    commands = cli_commands()
    relay = relay_routes()
    claimed_routes, claimed_commands, claimed_relay = set(), set(), set()
    for f in features():
        if not f.owns:
            errors.append(f"{f.id}: owns no code")
        if not f.reach:
            errors.append(f"{f.id}: no way in")
        if not f.proofs:
            errors.append(f"{f.id}: nothing proves it")
        app_facing = any(isinstance(r, (App, Label)) for r in f.reach)
        if app_facing and not f.flows:
            errors.append(f"{f.id}: an app feature without a live flow")
        for r in f.reach:
            if isinstance(r, App):
                claimed_routes.add(r.route)
                if r.route not in app_routes:
                    errors.append(f"{f.id}: route {r.route} isn't in app.dart")
                for tap in r.taps:
                    if not label_present(tap, source):
                        errors.append(f"{f.id}: label '{tap}' isn't in app/lib")
            elif isinstance(r, Label):
                if not label_present(r.text, source):
                    errors.append(f"{f.id}: label '{r.text}' isn't in app/lib")
            elif isinstance(r, Cli):
                claimed_commands.add(r.command)
                if r.command not in commands:
                    errors.append(f"{f.id}: `zafe {r.command}` doesn't exist")
            elif isinstance(r, RelayRoute):
                claimed_relay.add(r.path)
                if r.path not in relay:
                    errors.append(f"{f.id}: relay route {r.path} doesn't exist")
    for route in sorted(app_routes - claimed_routes):
        errors.append(f"route {route}: no feature reaches it")
    for command in sorted(commands - claimed_commands):
        errors.append(f"`zafe {command}`: no feature reaches it")
    for path in sorted(relay - claimed_relay):
        errors.append(f"relay route {path}: no feature reaches it")

    screens = {s.route: s for s in catalog.SCREENS}
    for route in sorted(app_routes - set(screens)):
        errors.append(f"route {route}: no Screen with a landmark in catalog.SCREENS")
    for route, screen in screens.items():
        if route not in app_routes:
            errors.append(f"Screen {route}: not a route in app.dart")
        if not label_present(screen.landmark, source):
            errors.append(f"Screen {route}: landmark '{screen.landmark}' isn't in app/lib")

    tests = rust_tests()
    ignored = ignored_tests()
    claimed_tests: set[tuple[str, str | None, str]] = set()
    flutter_files = flutter_test_files()
    claimed_flutter: set[str] = set()
    for owner, proofs in all_proofs():
        for p in proofs:
            if isinstance(p, Rust):
                known = tests.get((p.crate, p.target), set())
                if p.needs is not None and p.needs not in NEEDS:
                    errors.append(f"{owner}: unknown need {p.needs}")
                for name in p.names:
                    claimed_tests.add((p.crate, p.target, name))
                    if name not in known:
                        where = f"tests/{p.target}.rs" if p.target else "src"
                        errors.append(f"{owner}: {p.crate} {where} has no test `{name}`")
                    is_ignored = (p.crate, p.target, name) in ignored
                    if is_ignored and p.needs is None:
                        errors.append(f"{owner}: `{name}` is #[ignore]d; say what it needs")
                    if not is_ignored and p.needs is not None:
                        errors.append(f"{owner}: `{name}` runs by default; it needs nothing")
            elif isinstance(p, Flutter):
                claimed_flutter.add(p.file)
                if p.file not in flutter_files:
                    errors.append(f"{owner}: app/{p.file} doesn't exist")
            elif isinstance(p, Script):
                if not (ROOT / p.path).is_file():
                    errors.append(f"{owner}: {p.path} doesn't exist")
    for (crate, target), names in sorted(tests.items(), key=lambda kv: (kv[0][0], kv[0][1] or "")):
        for name in sorted(names):
            if (crate, target, name) not in claimed_tests:
                where = f"tests/{target}.rs" if target else "src"
                errors.append(f"{crate} {where}: test `{name}` proves no feature")
    for f in sorted(flutter_files - claimed_flutter):
        errors.append(f"app/{f}: proves no feature")

    defined, visits = flow_module()
    referenced = {flow for f in features() for flow in f.flows}
    for flow in sorted(referenced - defined):
        errors.append(f"flow {flow} is named by a feature but not written in flows.py")
    for flow in sorted(defined - referenced):
        errors.append(f"flow {flow} in flows.py proves no feature")
    for flow in catalog.ESSENTIAL_FLOWS:
        if flow not in defined:
            errors.append(f"essential flow {flow} isn't written")
    visited = set().union(*visits.values()) if visits else set()
    for route in sorted(app_routes - visited):
        errors.append(f"route {route}: no flow visits it")
    for flow, routes_seen in visits.items():
        for route in routes_seen - set(screens):
            errors.append(f"flow {flow}: visits {route}, which has no Screen")

    reachable = dart_reachable()
    for f in files:
        if f.startswith("app/lib/") and f.endswith(".dart") and f not in reachable:
            errors.append(f"{f}: main.dart never imports it")
    return errors

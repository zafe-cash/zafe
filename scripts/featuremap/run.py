import json
import subprocess
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from . import catalog
from .check import ROOT, features
from .model import Flutter, Rust, Script


@dataclass
class Result:
    proof: str
    status: str
    seconds: float
    detail: str = ""


def describe(p) -> str:
    if isinstance(p, Rust):
        where = f"tests/{p.target}" if p.target else "unit"
        return f"cargo {p.crate} {where}: {', '.join(p.names)}"
    if isinstance(p, Flutter):
        return f"flutter app/{p.file}"
    return f"script {p.path}"


def crate_kinds(crate: str) -> list[str]:
    src = ROOT / catalog.CRATES[crate] / "src"
    kinds = []
    if (src / "lib.rs").exists():
        kinds.append("--lib")
    if (src / "main.rs").exists():
        kinds.append("--bins")
    return kinds


def command(p) -> tuple[list[str], Path]:
    if isinstance(p, Rust):
        targets = ["--test", p.target] if p.target else crate_kinds(p.crate)
        flags = ["--exact", *p.names] + (["--include-ignored"] if p.needs else [])
        return ["cargo", "test", "-q", "-p", p.crate, *targets, "--", *flags], ROOT
    if isinstance(p, Flutter):
        return ["flutter", "test", p.file], ROOT / "app"
    runner = "python3" if p.path.endswith(".py") else "bash"
    return [runner, p.path], ROOT


def run_proof(p, needs: set[str]) -> Result:
    name = describe(p)
    if getattr(p, "needs", None) and p.needs not in needs:
        return Result(name, "skipped", 0.0, f"needs {p.needs}")
    cmd, cwd = command(p)
    started = time.monotonic()
    out = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    elapsed = round(time.monotonic() - started, 1)
    if out.returncode == 0:
        return Result(name, "pass", elapsed)
    tail = "\n".join((out.stdout + out.stderr).strip().splitlines()[-15:])
    return Result(name, "fail", elapsed, tail)


def select(ids: set[str]) -> list:
    return [f for f in features() if f.id in ids]


def verify(ids: set[str], needs: set[str], flows: list[str], log) -> dict:
    results: dict[str, list[Result]] = {}
    done: dict[str, Result] = {}
    for f in select(ids):
        for p in f.proofs:
            key = describe(p)
            if key not in done:
                log(f"  {key}")
                done[key] = run_proof(p, needs)
                log(f"    {done[key].status} ({done[key].seconds}s)")
            results.setdefault(f.id, []).append(done[key])
    flow_results = {}
    if flows:
        from . import flows as flow_module
        from .device import Session
        session = Session(log)
        for name in flows:
            log(f"  flow {name}")
            started = time.monotonic()
            try:
                getattr(flow_module, name)(session)
                flow_results[name] = Result(f"flow {name}", "pass", round(time.monotonic() - started, 1))
            except Exception as error:
                flow_results[name] = Result(f"flow {name}", "fail", round(time.monotonic() - started, 1), str(error))
            log(f"    {flow_results[name].status} ({flow_results[name].seconds}s)")
    return {"proofs": results, "flows": flow_results}


def record(path: Path, version: str, base: str | None, head: str, touched, outcome: dict):
    commit = subprocess.run(["git", "rev-parse", head], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    data = {
        "version": version,
        "commit": commit,
        "base": base,
        "date": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "touched": sorted(touched.features) if touched else None,
        "plumbing_touched": sorted(touched.plumbing) if touched else None,
        "features": [
            {
                "id": f.id,
                "area": area.id,
                "name": f.name,
                "proofs": [r.__dict__ for r in outcome["proofs"].get(f.id, [])],
                "flows": [outcome["flows"][n].__dict__ for n in f.flows if n in outcome["flows"]],
            }
            for area in catalog.AREAS for f in area.features
        ],
    }
    path.write_text(json.dumps(data, indent=2) + "\n")

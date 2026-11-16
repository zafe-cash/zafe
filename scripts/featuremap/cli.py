import argparse
import sys
from pathlib import Path

from . import catalog
from .check import ROOT, by_hand, check, features
from .run import record, verify
from .touched import touched


def log(line: str):
    print(line, flush=True)


def cmd_check(_args) -> int:
    errors = check()
    for e in errors:
        print(e)
    if errors:
        print(f"\n{len(errors)} problems: the feature map and the code disagree.")
        return 1
    print("the feature map matches the code")
    return 0


def split_range(spec: str) -> tuple[str, str]:
    base, _, head = spec.partition("..")
    return base, head or "HEAD"


def cmd_touched(args) -> int:
    base, head = split_range(args.range)
    t = touched(base, head)
    names = {f.id: f.name for f in features()}
    for fid in sorted(t.features):
        print(f"{fid}  {names[fid]}")
    for p in sorted(t.plumbing):
        print(f"plumbing  {p}")
    for path in sorted(t.unowned):
        print(f"outside the map  {path}")
    return 0


def cmd_verify(args) -> int:
    all_ids = {f.id for f in features()}
    t, base, head = None, None, "HEAD"
    if args.essential:
        ids, flows = set(), list(catalog.ESSENTIAL_FLOWS)
    elif args.range:
        base, head = split_range(args.range)
        t = touched(base, head)
        ids = all_ids if t.plumbing else set(t.features)
        flows = sorted({flow for f in features() if f.id in ids for flow in f.flows}) if args.flows else []
        if args.flows and t.plumbing:
            flows = sorted(set(flows) | set(catalog.ESSENTIAL_FLOWS))
    elif args.feature:
        ids = set(args.feature) & all_ids
        flows = sorted({flow for f in features() if f.id in ids for flow in f.flows}) if args.flows else []
    else:
        ids = all_ids
        flows = sorted({flow for f in features() for flow in f.flows}) if args.flows else []
    needs = set(args.needs.split(",")) if args.needs else set()
    log(f"{len(ids)} features, {len(flows)} flows")
    outcome = verify(ids, needs, flows, log)
    if args.record:
        path = ROOT / "docs/releases" / f"{args.record}.features.json"
        record(path, args.record, base, head, t, outcome)
        log(f"recorded {path.relative_to(ROOT)}")
    failed = [r for rs in outcome["proofs"].values() for r in rs if r.status == "fail"]
    failed += [r for r in outcome["flows"].values() if r.status == "fail"]
    skipped = {r.proof: r.detail for rs in outcome["proofs"].values() for r in rs if r.status == "skipped"}
    for r in failed:
        log(f"FAILED {r.proof}\n{r.detail}")
    for proof, why in sorted(skipped.items()):
        log(f"skipped {proof} ({why})")
    direct = set(t.features) if t else ids
    for f in features():
        if f.id in direct and by_hand(f):
            log(f"check on a phone: {f.name}")
    return 1 if failed else 0


def cmd_flow(args) -> int:
    from . import flows
    from .device import Session
    session = Session(log)
    for name in args.names:
        log(f"flow {name}")
        getattr(flows, name)(session)
        log(f"  pass")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="features.py")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    p = sub.add_parser("touched")
    p.add_argument("range")
    p = sub.add_parser("verify")
    p.add_argument("--range")
    p.add_argument("--feature", action="append")
    p.add_argument("--essential", action="store_true")
    p.add_argument("--flows", action="store_true")
    p.add_argument("--needs")
    p.add_argument("--record")
    p = sub.add_parser("flow")
    p.add_argument("names", nargs="+")
    args = parser.parse_args(argv)
    commands = {"check": cmd_check, "touched": cmd_touched, "verify": cmd_verify, "flow": cmd_flow}
    return commands[args.command](args)

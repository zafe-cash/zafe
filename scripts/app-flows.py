import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from appflows import flows
from appflows.device import Session


def main(names: list[str]) -> int:
    unknown = [n for n in names if n not in flows.FLOWS]
    if unknown:
        print(f"unknown flows: {', '.join(unknown)}; known: {', '.join(flows.FLOWS)}")
        return 2
    session = Session(print)
    failed = []
    for name in names or flows.FLOWS:
        print(f"flow {name}", flush=True)
        started = time.monotonic()
        try:
            getattr(flows, name)(session)
            print(f"  pass ({time.monotonic() - started:.1f}s)", flush=True)
        except Exception as error:
            print(f"  fail ({time.monotonic() - started:.1f}s)\n{error}", flush=True)
            failed.append(name)
    if failed:
        print(f"FAILED: {', '.join(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

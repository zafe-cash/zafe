import json
import re
import shutil
import subprocess
import time
from pathlib import Path

from .device import ROOT, FlowError, Session

FLOWS = ("join_vault", "receive", "propose_from_app", "approve_cli_payment", "backup_restore", "create_vault")

REGTEST = ROOT / "infra/regtest"
NOTES = 6
NOTE_ZEC = 5


def _chain_up(s: Session) -> bool:
    out = subprocess.run(["ths", "--name", "zafe-regtest", "endpoints"], capture_output=True, text=True)
    return out.returncode == 0 and "9067" in out.stdout


def _start_chain(s: Session):
    if not _chain_up(s):
        subprocess.run(["bash", str(REGTEST / "up.sh")], check=True, capture_output=True, timeout=900)


def _address(s: Session) -> str:
    return re.search(r"^address (\S+)", s.h.cli("B", "vault", "show"), re.M).group(1)


def _fund(s: Session, notes: int = NOTES):
    _start_chain(s)
    subprocess.run(["bash", str(REGTEST / "fund.sh"), _address(s), str(notes)], check=True,
                   capture_output=True, timeout=900)


def _vault(s: Session):
    if not s.state.get("vault"):
        join_vault(s)
    s.app.home()


def _funded(s: Session):
    _vault(s)
    if not s.state.get("funded"):
        _fund(s)
        s.app.wait(f"{NOTES * NOTE_ZEC} TAZ", timeout=240)
        s.state["funded"] = True


def join_vault(s: Session):
    app, h = s.app, s.h
    h.run("stop")
    _start_chain(s)
    h.run("start")
    app.fresh()
    app.on("/welcome")
    app.tap("Join with an invite")
    app.on("/join")
    app.fill(h.invite())
    app.tap("Join vault")
    app.tap("Not now")
    h.run("join-all")
    h.run("seal")
    words = re.search(r"^safety words: (.+)$", h.run("keygen", timeout=600), re.M).group(1).split()
    shown = app.wait("Safety words", timeout=120)
    deadline = time.monotonic() + 120
    while not all(w in shown.label for w in words):
        if time.monotonic() > deadline:
            raise FlowError(f"the app's words aren't the CLI's {words}: {shown.label}")
        time.sleep(1)
        shown = app.wait("Safety words")
    app.tap("They match")
    app.on("/backup-prompt", timeout=300)
    app.tap("Later")
    app.on("/home", timeout=60)
    s.state.update(vault=True, funded=False)


def receive(s: Session):
    _vault(s)
    app = s.app
    app.on("/home")
    app.tap("Receive")
    app.on("/receive")
    address = _address(s)
    app.wait(f"{address[:13]} ... {address[-11:]}")
    app.back()
    _funded(s)
    app.tap("Received payment")
    app.on("/received/:txid")
    app.wait(f"+{NOTE_ZEC}.00 TAZ")
    app.back()
    app.on("/home")


def _payee(s: Session, account: int = 2) -> tuple[str, int]:
    out = subprocess.run(["ths", "--name", "zafe-regtest", "wallet", "balances", "--json"], capture_output=True,
                         text=True, check=True).stdout
    entry = json.loads(out)[account - 1]
    return entry["unified_address"], entry["ironwood_zatoshi"]


def _proposal_id(s: Session, zat: int) -> str:
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        for line in s.h.cli("B", "proposals").splitlines():
            m = re.match(rf"^([0-9a-f]{{32}}) Open {zat} zat", line)
            if m:
                return m.group(1)
        time.sleep(3)
    raise FlowError(f"no open proposal of {zat} zat reached member B")


def _paid(s: Session, before: int, zat: int):
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        s.h.mine(1)
        if _payee(s)[1] >= before + zat:
            return
        time.sleep(3)
    raise FlowError(f"the payee never received {zat} zat")


def propose_from_app(s: Session):
    _funded(s)
    app = s.app
    payee, before = _payee(s)
    app.on("/home")
    app.tap("New payment")
    app.on("/send")
    app.fill(payee)
    app.tap("Continue")
    app.wait("Enter amount")
    app.fill("1.5")
    app.tap("Add short encrypted message")
    app.fill("flow check")
    app.tap("Save message")
    app.tap("Finish & review")
    app.wait("Review payment")
    app.wait("flow check")
    app.tap("Propose payment")
    app.on("/proposal/:id", timeout=120)
    app.wait("Approved by You")
    proposal = _proposal_id(s, 150_000_000)
    s.h.cli("B", "approve", proposal)
    app.wait("Ready to send", timeout=180)
    app.tap("Send now")
    app.on("/proposal/:id/send", timeout=60)
    app.wait("Sent!", timeout=240)
    _paid(s, before, 150_000_000)
    app.on("/home", timeout=60)


def _row(s: Session, text: str, status: str, timeout: float = 180):
    deadline = time.monotonic() + timeout
    while True:
        row = s.app.wait(text, timeout=timeout)
        if status in row.label:
            return
        if time.monotonic() > deadline:
            raise FlowError(f"'{text}' never became '{status}': {row.label}")
        time.sleep(3)


def approve_cli_payment(s: Session):
    _funded(s)
    app, h = s.app, s.h
    payee, before = _payee(s)
    proposal = re.search(r"proposal ([0-9a-f]{32})",
                         h.cli("B", "propose", "--to", payee, "--amount", "50000000", "--memo", "from B")).group(1)
    app.on("/home")
    app.wait("Needs you", timeout=120)
    app.tap("Approve")
    app.on("/proposal/:id")
    app.wait("Checked on this phone", timeout=60)
    app.tap("Approve")
    app.on("/home", timeout=60)
    app.wait("1 of 2 approved", timeout=60)
    h.cli("B", "approve", proposal)
    h.cli("B", "send", proposal)
    _paid(s, before, 50_000_000)
    _row(s, "0.5 TAZ", "Sent")


def backup_restore(s: Session):
    _funded(s)
    app, h = s.app, s.h
    app.on("/home")
    app.tap("Settings")
    app.on("/settings")
    app.tap("Back up")
    app.on("/export")
    words = re.findall(r"\d+\s+([a-z]+)", app.wait("1  ").label)
    if len(words) != 12:
        raise FlowError(f"expected 12 backup words, read {words}")
    app.tap("I saved them")
    app.wait("Check your words")
    for i, k in enumerate(int(n) for n in re.findall(r"Word (\d+)", app.screen())):
        app.fill(words[k - 1], i)
    app.tap("Create backup")
    app.wait("Backup ready", timeout=60)
    app.tap("Copy as text")
    app.fresh()
    app.on("/welcome")
    app.tap("I'm already in a vault")
    app.tap("Yes, restore it")
    app.on("/restore")
    app.tap("Paste")
    app.fill(" ".join(words), 1)
    app.tap("Restore vault")
    app.tap("Not now", timeout=120)
    app.on("/home", timeout=120)
    app.gone("— TAZ", timeout=240)
    payee, before = _payee(s)
    proposal = re.search(r"proposal ([0-9a-f]{32})",
                         h.cli("B", "propose", "--to", payee, "--amount", "20000000", "--memo", "after restore")).group(1)
    app.wait("Needs you", timeout=120)
    app.tap("Approve")
    app.on("/proposal/:id")
    app.wait("Checked on this phone", timeout=60)
    app.tap("Approve")
    app.on("/home", timeout=60)
    h.cli("B", "approve", proposal)
    h.cli("B", "send", proposal)
    _paid(s, before, 20_000_000)


def _member(s: Session, who: str):
    shutil.rmtree(Path(s.h.env.get("ZAFE_HARNESS_DIR", Path.home() / ".cache/zafe-harness")) / who, ignore_errors=True)
    s.h.cli(who, "init")


def create_vault(s: Session):
    _vault(s)
    app, h = s.app, s.h
    for who in ("D", "E"):
        _member(s, who)
    app.tap("Vault picture")
    app.tap("Add vault")
    app.on("/welcome")
    app.tap("Create a vault")
    app.on("/create")
    app.fill("Payroll", 0)
    app.fill("Ana", 1)
    app.tap("Create vault")
    app.wait("Invite members", timeout=60)
    app.tap("Copy")
    invite = app.clipboard().split("invite=")[-1]
    for who in ("D", "E"):
        h.cli(who, "vault", "join", invite)
    waits = [subprocess.Popen(["bash", str(ROOT / "scripts/app-harness.sh"), "cli", who, "vault", "members", "--wait"],
                              cwd=ROOT, stdout=subprocess.PIPE, text=True) for who in ("D", "E")]
    outputs = [w.communicate(timeout=300)[0] for w in waits]
    words = {re.search(r"^safety words: (.+)$", o, re.M).group(1) for o in outputs}
    if len(words) != 1:
        raise FlowError(f"CLI members disagree on the safety words: {words}")
    phrase = words.pop()
    app.wait("Safety words", timeout=120)
    app.gone("Waiting for", timeout=120)
    shown = app.wait("Safety words")
    if not all(w in shown.label for w in phrase.split()):
        raise FlowError(f"the app's words aren't the CLI's {phrase}: {shown.label}")
    keygens = [subprocess.Popen(["bash", str(ROOT / "scripts/app-harness.sh"), "cli", who, "vault", "keygen",
                                 "--safety-number", phrase], cwd=ROOT, stdout=subprocess.PIPE, text=True)
               for who in ("D", "E")]
    app.tap("They match")
    app.on("/backup-prompt", timeout=300)
    app.tap("Later")
    app.on("/home", timeout=60)
    created = [k.communicate(timeout=300)[0] for k in keygens]
    address = re.search(r"vault created: (\S+)", created[0]).group(1)
    app.wait("Payroll")
    app.tap("Receive")
    app.on("/receive")
    app.wait(f"{address[:13]} ... {address[-11:]}")
    app.back()
    s.state["second_vault"] = "Payroll"

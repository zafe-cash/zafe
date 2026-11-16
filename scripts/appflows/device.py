import os
import re
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import catalog
from .check import ROOT

PACKAGE = "xyz.zafe.zafe"
READER = "xyz.zafe.zafe.testnet"
HARNESS = ROOT / "scripts/app-harness.sh"
SCREENS = {s.route: s.landmark for s in catalog.SCREENS}
NODE = re.compile(r'^\s*(@e\d+(?:~s\d+)?) \[([^\]]+)\](.*)$')
FLAGS = re.compile(r'((?: \[[a-z-]+\])*)$')


class FlowError(Exception):
    pass


@dataclass
class Node:
    ref: str
    role: str
    label: str
    flags: str


def agent(*args: str, timeout: int = 60) -> str:
    out = subprocess.run(["agent-device", *args], cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    if out.returncode != 0:
        raise FlowError(f"agent-device {' '.join(args)}: {(out.stdout + out.stderr).strip()[-400:]}")
    return out.stdout


def adb(*args: str) -> str:
    return subprocess.run(["adb", *args], capture_output=True, text=True, check=True).stdout


class Device:
    def __init__(self, log):
        self.log = log

    def fresh(self):
        adb("shell", "pm", "clear", PACKAGE)
        self.open()

    def open(self):
        agent("open", PACKAGE, "--foreground", "--relaunch", timeout=120)

    def nodes(self) -> list[Node]:
        text = agent("snapshot", "-i")
        found, current = [], None
        for line in text.splitlines():
            m = NODE.match(line)
            if m:
                rest = m.group(3)
                flags = FLAGS.search(rest).group(1)
                label = rest[:len(rest) - len(flags)].strip()
                current = Node(m.group(1), m.group(2), label[1:].rstrip('"') if label.startswith('"') else label, flags)
                found.append(current)
            elif current is not None and not line.startswith(("Page:", "App:", "Snapshot:", "  [")):
                flags = FLAGS.search(line).group(1)
                if flags:
                    current.flags += flags
                current.label += "\n" + line[:len(line) - len(flags)].rstrip().rstrip('"')
        return found

    def screen(self) -> str:
        return "\n".join(n.label for n in self.nodes())

    def wait(self, text: str, timeout: float = 30) -> Node:
        deadline = time.monotonic() + timeout
        while True:
            for n in self.nodes():
                if text in n.label:
                    return n
            if time.monotonic() > deadline:
                raise FlowError(f"'{text}' never showed; screen: {self.screen()[:600]}")
            time.sleep(1)

    def gone(self, text: str, timeout: float = 30):
        deadline = time.monotonic() + timeout
        while any(text in n.label for n in self.nodes()):
            if time.monotonic() > deadline:
                raise FlowError(f"'{text}' never went away")
            time.sleep(1)

    def on(self, route: str, timeout: float = 30):
        self.wait(SCREENS[route], timeout)

    def find(self, label: str, timeout: float = 30) -> Node:
        deadline = time.monotonic() + timeout
        while True:
            nodes = self.nodes()
            exact = [n for n in nodes if n.label.split("\n")[0] == label]
            loose = [n for n in nodes if label in n.label]
            hit = (exact or loose or [None])[0]
            if hit and "[disabled]" not in hit.flags:
                return hit
            if time.monotonic() > deadline:
                raise FlowError(f"no enabled '{label}' to tap; screen: {self.screen()[:600]}")
            time.sleep(1)

    def tap(self, label: str, timeout: float = 30):
        deadline = time.monotonic() + timeout
        while True:
            for args in (("press", f'label="{label}"'), ("find", label, "click", "--first")):
                try:
                    agent(*args)
                    time.sleep(0.6)
                    return
                except FlowError as error:
                    last = error
            try:
                agent("scroll", "down", "--until", f'label="{label}"', timeout=60)
                continue
            except FlowError:
                pass
            if time.monotonic() > deadline:
                raise FlowError(f"couldn't tap '{label}': {last}; screen: {self.screen()[:600]}")
            time.sleep(1)

    def fields(self) -> list[Node]:
        return [n for n in self.nodes() if n.role == "text-field"]

    def fill(self, text: str, index: int = 0):
        deadline = time.monotonic() + 20
        while len(self.fields()) <= index:
            if time.monotonic() > deadline:
                raise FlowError(f"no text field #{index}")
            time.sleep(1)
        agent("fill", self.fields()[index].ref, text)
        time.sleep(0.4)

    def back(self):
        agent("back")
        time.sleep(0.6)

    def home(self):
        self.open()
        self.on("/home", timeout=60)

    def scroll_to(self, text: str):
        agent("scroll", "down", "--until", f'label="{text}"', timeout=90)

    def link(self, uri: str):
        adb("shell", "am", "start", "-a", "android.intent.action.VIEW", "-d", uri, PACKAGE)
        time.sleep(1.5)

    def clipboard(self) -> str:
        agent("open", READER, "--foreground")
        time.sleep(2)
        if any(n.label == "Dismiss copied text" for n in self.nodes()):
            self.tap("Dismiss copied text")
        if not any("Join with an invite" in n.label for n in self.nodes()):
            if any(n.label == "Back" for n in self.nodes()):
                self.back()
            self.tap("Vault picture")
            self.tap("Add vault")
        self.tap("Join with an invite")
        self.wait("Join a vault")
        self.tap("Paste")
        field = self.fields()[0]
        text = agent("get", "text", field.ref).strip()
        self.back()
        agent("open", PACKAGE, "--foreground")
        time.sleep(1.5)
        return text

    def background(self):
        adb("shell", "input", "keyevent", "KEYCODE_HOME")
        time.sleep(1)


@dataclass
class Harness:
    log: object
    env: dict = field(default_factory=lambda: dict(os.environ))

    def run(self, *args: str, timeout: int = 900) -> str:
        out = subprocess.run(["bash", str(HARNESS), *args], cwd=ROOT, capture_output=True, text=True,
                             timeout=timeout, env=self.env)
        if out.returncode != 0:
            raise FlowError(f"app-harness {' '.join(args)}: {(out.stdout + out.stderr).strip()[-600:]}")
        return out.stdout

    def cli(self, who: str, *args: str, timeout: int = 900) -> str:
        return self.run("cli", who, *args, timeout=timeout)

    def invite(self) -> str:
        return (Path(self.env.get("ZAFE_HARNESS_DIR", Path.home() / ".cache/zafe-harness")) / "invite").read_text().strip().splitlines()[-1]

    def mine(self, blocks: int):
        self.run("mine", str(blocks))


class Session:
    def __init__(self, log):
        self.log = log
        self.app = Device(log)
        self.h = Harness(log)
        self.state: dict[str, object] = {}

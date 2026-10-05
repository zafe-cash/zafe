#!/usr/bin/env python3
"""Patina two-tone icons (docs/brand.md §4) from Phosphor Icons (MIT).

For each AppIcons name in MAP, writes into app/assets/icons/patina/:
  <name>.svg         idle: Phosphor duotone. The outline is black (#000000) and
                     thickened from 16 to 18 units (1.7 px at 24 px) with a round
                     stroke; the duotone layer is magenta (#FF00FF). AppIcon's
                     colour mapper swaps black for the icon colour and magenta for
                     the brand accent at ~38%.
  <name>_active.svg  active: Phosphor fill, black, drawn solid in the accent.
and copies Phosphor's licence to app/assets/icons/licenses/Phosphor-MIT.txt.

Usage: scripts/brand/patina_icons.py [path/to/phosphor-core/package]
Without a path it runs `npm pack @phosphor-icons/core@2.1.1` in a temp dir.

Keep MAP in sync with `AppIcons` in app/lib/src/core/widgets/app_icon.dart.
"""
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "app/assets/icons/patina"
LICENSES = ROOT / "app/assets/icons/licenses"
VERSION = "2.1.1"

# AppIcons name -> Phosphor icon. Every AppIcons name except the marks
# (zcash_currency) and the code-drawn loader.
MAP = {
    "add_new": "plus-circle",
    "arrow_down": "arrow-down",
    "arrow_down_circle": "arrow-circle-down",
    "book": "book-open",
    "camera_denied": "camera-slash",
    "cancel": "minus",
    "check": "check",
    "check_circle": "check-circle",
    "chevron_backward": "caret-left",
    "chevron_forward": "caret-right",
    "cog": "gear-six",
    "copy": "copy",
    "cross": "x",
    "day": "sun",
    "edit": "pencil-line",
    "edit_filled": "pencil-simple",
    "endpoint": "plugs-connected",
    "eye": "eye",
    "eye_closed": "eye-slash",
    "globe": "globe",
    "help": "question",
    "history": "clock-counter-clockwise",
    "home": "house",
    "import_wallet": "clipboard-text",
    "key": "key",
    "link": "link",
    "lock": "lock",
    "monitor": "monitor",
    "night": "moon",
    "plane": "paper-plane-tilt",
    "plus": "plus",
    "qr": "qr-code",
    "renew": "arrows-clockwise",
    "share": "export",
    "shield_keyhole": "shield-check",
    "theme": "circle-half",
    "time": "clock",
    "sound": "speaker-high",
    "sound_off": "speaker-slash",
    "tor": "detective",
    "trash": "trash",
    "unlock": "lock-open",
    "user": "user",
    "users": "users-three",
    "wallet": "wallet",
    "warning": "warning",
    "warning_circle": "warning-circle",
}

# Line icons: Phosphor's duotone for these is a different drawing (a closed triangle for
# the carets) or adds a backdrop (a square behind the x, check, plus and minus, a disc
# behind the arrows), so they use the
# regular weight, outline only.
LINE_ONLY = {"cross", "check", "chevron_forward", "chevron_backward", "arrow_down",
             "plus", "cancel", "renew"}

OUTLINE = 'fill="#000000" stroke="#000000" stroke-width="2" stroke-linejoin="round"'


def phosphor_dir() -> pathlib.Path:
    if len(sys.argv) > 1:
        return pathlib.Path(sys.argv[1])
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="phosphor-"))
    subprocess.run(["npm", "pack", f"@phosphor-icons/core@{VERSION}"], cwd=tmp, check=True,
                   stdout=subprocess.DEVNULL)
    with tarfile.open(next(tmp.glob("*.tgz"))) as tar:
        tar.extractall(tmp, filter="data")
    return tmp / "package"


def idle(svg: str, line_only: bool) -> str:
    svg = svg.replace(' fill="currentColor"', "", 1)
    # The duotone layer carries opacity="0.2"; everything else is outline.
    layer = "" if line_only else r'<path d="\1" fill="#FF00FF"/>'
    svg = re.sub(r'<path d="([^"]*)" opacity="0.2"/>', layer, svg)
    return re.sub(r'<path d="([^"]*)"/>', rf'<path d="\1" {OUTLINE}/>', svg)


def active(svg: str) -> str:
    return svg.replace('fill="currentColor"', 'fill="#000000"', 1)


def main() -> None:
    src = phosphor_dir()
    OUT.mkdir(parents=True, exist_ok=True)
    LICENSES.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.svg"):
        old.unlink()
    for name, ph in MAP.items():
        line_only = name in LINE_ONLY
        duo = (src / (f"assets/regular/{ph}.svg" if line_only
                      else f"assets/duotone/{ph}-duotone.svg")).read_text()
        fill = (src / f"assets/fill/{ph}-fill.svg").read_text()
        (OUT / f"{name}.svg").write_text(idle(duo, line_only) + "\n")
        (OUT / f"{name}_active.svg").write_text(active(fill) + "\n")
    shutil.copy(src / "LICENSE", LICENSES / "Phosphor-MIT.txt")
    print(f"wrote {len(MAP) * 2} icons to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

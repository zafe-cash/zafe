#!/usr/bin/env python3
"""Checks that the website docs (infra/site) still match the app and the server.

  * Every app label a docs page names (`<strong>…</strong>` in
    infra/site/src/pages/docs/*.astro and infra/site/src/docs/*.ts) appears as text in
    the app's Dart code (app/lib, generated bridge code excluded). `<b>` is plain
    emphasis and isn't checked; a `…` in a label stands for a name or value and matches
    anything ("Help instead of …").
  * Every server setting the docs mention (`ZAFE_RELAY_*`) is read by the relay
    (crates/zafe-relay, infra/relay).
  * Every screen a page shows (`<Shot name="x">`) exists as
    infra/site/public/assets/screens/x_light.webp and is listed in infra/site/assets.py,
    so it is regenerated from the app's own renders.

Run with scripts/check_docs_links.py (links from the app to the site). Rename a button,
a screen or a setting, and this fails until the docs say the same.

Usage: scripts/check_docs_sync.py   (exit 1 on findings)
"""
import html
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SITE = ROOT / "infra/site"
DOC_SOURCES = sorted((SITE / "src/pages/docs").glob("*.astro")) + sorted(
    (SITE / "src/docs").glob("*.ts")
)


def norm(text):
    """One spelling for quotes and spaces, so ’ in the docs matches ' in Dart."""
    text = text.replace("’", "'").replace("‘", "'").replace("\\'", "'")
    return re.sub(r"\s+", " ", text).strip()


def app_text():
    files = [
        p for p in (ROOT / "app/lib").rglob("*.dart") if "/rust/" not in p.as_posix()
    ]
    return norm("\n".join(p.read_text() for p in files))


def labels():
    """(label, file, line) for every <strong> in the docs sources."""
    for path in DOC_SOURCES:
        text = path.read_text()
        for m in re.finditer(r"<strong>(.*?)</strong>", text, re.S):
            raw = m.group(1)
            if "{" in raw:  # an expression (`{p.title}`), not a label
                continue
            label = norm(html.unescape(re.sub(r"<[^>]+>", "", raw)))
            if label:
                yield label, path, text.count("\n", 0, m.start()) + 1


def found(label, app):
    if "…" not in label:
        return label in app
    parts = [re.escape(p.strip()) for p in label.split("…")]
    return re.search(r".{0,40}".join(parts), app) is not None


def main():
    findings = []
    app = app_text()
    for label, path, line in labels():
        if not found(label, app):
            findings.append(
                f"{path.relative_to(ROOT)}:{line}: app label “{label}” isn't in app/lib "
                "(renamed? update the docs; plain emphasis is <b>)"
            )

    relay = "\n".join(
        p.read_text(errors="ignore")
        for base in (ROOT / "crates/zafe-relay", ROOT / "infra/relay")
        for p in base.rglob("*")
        if p.is_file() and p.suffix in {".rs", ".yml", ".yaml", ".sh", ".env", ""}
    )
    for path in DOC_SOURCES:
        text = path.read_text()
        for m in re.finditer(r"\bZAFE_RELAY_[A-Z0-9_]+\b", text):
            name = m.group(0)
            if name not in relay:
                line = text.count("\n", 0, m.start()) + 1
                findings.append(
                    f"{path.relative_to(ROOT)}:{line}: server setting {name} isn't read by "
                    "crates/zafe-relay or infra/relay"
                )

    assets = (SITE / "assets.py").read_text()
    for path in DOC_SOURCES:
        text = path.read_text()
        for m in re.finditer(r'<Shot\b[^>]*\bname="([^"]+)"', text, re.S):
            name = m.group(1)
            line = text.count("\n", 0, m.start()) + 1
            where = f"{path.relative_to(ROOT)}:{line}"
            if not (SITE / f"public/assets/screens/{name}_light.webp").exists():
                findings.append(f"{where}: screen {name}_light.webp is missing")
            if f'"{name}"' not in assets:
                findings.append(
                    f"{where}: screen {name} isn't in infra/site/assets.py (can't be "
                    "regenerated from the app's renders)"
                )

    for f in findings:
        print(f)
    if findings:
        print(f"\n{len(findings)} place(s) where the docs and the app differ.", file=sys.stderr)
        return 1
    print("docs match the app")
    return 0


if __name__ == "__main__":
    sys.exit(main())

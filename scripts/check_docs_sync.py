#!/usr/bin/env python3
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
    text = text.replace("’", "'").replace("‘", "'").replace("\\'", "'")
    return re.sub(r"\s+", " ", text).strip()


def app_text():
    files = [
        p for p in (ROOT / "app/lib").rglob("*.dart") if "/rust/" not in p.as_posix()
    ]
    return norm("\n".join(p.read_text() for p in files))


def labels():
    for path in DOC_SOURCES:
        text = path.read_text()
        for m in re.finditer(r"<strong>(.*?)</strong>", text, re.S):
            raw = m.group(1)
            if "{" in raw:
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

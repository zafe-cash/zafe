#!/usr/bin/env python3
"""Checks that every website link the app keeps in docs_links.dart still resolves.

    infra/site/build.sh && python3 scripts/check_docs_links.py

Reads the string constants in app/lib/src/core/config/docs_links.dart (following `$kName`
interpolations), takes each `https://zafe.cash/...` URL and checks that the built site
(infra/site/dist) has the page (build.format 'file': /docs/x -> docs/x.html), or that
infra/site/public/_redirects sends it to a page that exists. A `#fragment` must be an id
on that page. Also checks that each redirect's target exists, since shipped builds still
use the old paths (/relay). Exits 1 on the first list of failures.
"""

import os
import re
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), ".."))
LINKS = os.path.join(ROOT, "app", "lib", "src", "core", "config", "docs_links.dart")
DIST = os.path.join(ROOT, "infra", "site", "dist")
REDIRECTS = os.path.join(ROOT, "infra", "site", "public", "_redirects")
HOST = "https://zafe.cash"


def constants(source):
    """`const kName = '...';` values, with `$kOther` / `${kOther}` filled in."""
    raw = dict(re.findall(r"const\s+(k\w+)\s*=\s*'([^']*)'\s*;", source))
    def resolve(value, seen=()):
        def sub(m):
            name = m.group(1) or m.group(2)
            if name in seen or name not in raw:
                raise SystemExit(f"docs_links.dart: can't resolve ${name}")
            return resolve(raw[name], seen + (name,))
        return re.sub(r"\$\{(k\w+)\}|\$(k\w+)", sub, value)
    return {name: resolve(value, (name,)) for name, value in raw.items()}


def redirects():
    out = {}
    if os.path.exists(REDIRECTS):
        for line in open(REDIRECTS, encoding="utf-8"):
            parts = line.split()
            if len(parts) >= 2 and not parts[0].startswith("#"):
                out[parts[0].rstrip("/") or "/"] = parts[1]
    return out


def page_file(path):
    """The built file serving `path`, or None."""
    path = path.rstrip("/") or "/"
    rel = path.lstrip("/")
    candidates = ["index.html"] if path == "/" else [f"{rel}.html", os.path.join(rel, "index.html"), rel]
    for c in candidates:
        f = os.path.join(DIST, c)
        if os.path.isfile(f):
            return f
    return None


def check(url, moved):
    if not url.startswith(HOST + "/") and url != HOST:
        return f"{url}: not on {HOST}"
    rest = url[len(HOST):] or "/"
    path, _, fragment = rest.partition("#")
    target = path
    if page_file(path) is None and (path.rstrip("/") or "/") in moved:
        target = moved[path.rstrip("/") or "/"]
    f = page_file(target)
    if f is None:
        return f"{url}: no page at {target} in infra/site/dist"
    if fragment and not re.search(rf'\sid="{re.escape(fragment)}"', open(f, encoding="utf-8").read()):
        return f"{url}: no id=\"{fragment}\" on {target}"
    return None


def main():
    if not os.path.isdir(DIST):
        sys.exit("no infra/site/dist: run infra/site/build.sh first")
    values = constants(open(LINKS, encoding="utf-8").read())
    urls = sorted({v for v in values.values() if v.startswith(HOST + "/")})
    if not urls:
        sys.exit("docs_links.dart: no https://zafe.cash/... links found")
    moved = redirects()
    errors = [e for e in (check(u, moved) for u in urls) if e]
    errors += [f"redirect {src} -> {dst}: no such page" for src, dst in moved.items() if page_file(dst) is None]
    for u in urls:
        print(("FAIL " if any(e.startswith(u + ":") for e in errors) else "ok   ") + u)
    for src, dst in moved.items():
        print(f"ok   redirect {src} -> {dst}" if page_file(dst) else f"FAIL redirect {src} -> {dst}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

"""Promotes a build from Google Play's internal track to production, with its in-app update
priority and notes. Run by .github/workflows/play-promote.yml; never by hand.

    PLAY_KEY=<service account JSON> python3 scripts/play_promote.py <version> <rollout percent> [--dry-run]

--dry-run does everything but the change: it opens an edit, reads the tracks, finds the
build, then deletes the edit (a check that the key and its Play permissions work).

The build is the internal-track release named <version> (release-mainnet.yml uploads each
mainnet tag there under that name), so production only ever gets a bundle that was tested
first. `docs/releases/<version>.md` gives the class: essential = priority 5 and 100% at
once (members of a vault update together); optional = priority 2, any rollout. A new app's
first production release can't be staged (Play refuses it).
"""

import json
import os
import re
import sys

import requests
from google.auth.transport.requests import Request
from google.oauth2 import service_account

PACKAGE = "xyz.zafe.zafe"
API = f"https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{PACKAGE}"


def fail(msg):
    print(f"::error::{msg}")
    sys.exit(1)


def release_file(version):
    path = os.path.join(os.path.dirname(__file__), "..", "docs", "releases", f"{version}.md")
    try:
        text = open(path).read()
    except FileNotFoundError:
        fail(f"docs/releases/{version}.md is missing")
    m = re.match(r"class: *(optional|essential)\s*\n(.*)", text, re.S)
    if not m:
        fail(f"docs/releases/{version}.md must start with 'class: optional|essential'")
    notes = m.group(2).strip()
    if not 0 < len(notes) <= 500:
        fail(f"docs/releases/{version}.md: notes must be 1-500 characters")
    return m.group(1), notes


def main():
    dry = "--dry-run" in sys.argv[3:]
    if len(sys.argv) not in (3, 4) or not re.fullmatch(r"\d+\.\d+\.\d+", sys.argv[1]):
        fail("usage: play_promote.py <version like 1.2.3> <rollout percent> [--dry-run]")
    version, percent = sys.argv[1], float(sys.argv[2])
    if not 0 < percent <= 100:
        fail("rollout percent must be in (0, 100]")
    cls, notes = release_file(version)
    priority = 5 if cls == "essential" else 2
    if cls == "essential" and percent != 100:
        fail("an essential release goes to 100%: members of a vault update together")

    creds = service_account.Credentials.from_service_account_info(
        json.loads(os.environ["PLAY_KEY"]),
        scopes=["https://www.googleapis.com/auth/androidpublisher"])
    creds.refresh(Request())
    s = requests.Session()
    s.headers["Authorization"] = f"Bearer {creds.token}"

    def call(method, path, **kw):
        r = s.request(method, API + path, timeout=60, **kw)
        if r.status_code >= 300:
            fail(f"{method} {path}: {r.status_code} {r.text[:400]}")
        return r.json() if r.text else {}

    edit = call("POST", "/edits")["id"]
    internal = call("GET", f"/edits/{edit}/tracks/internal")
    # CI names releases "<version>"; the Console's default is "<code> (<version>)".
    names = [r.get("name", "") for r in internal.get("releases", [])]
    codes = [c for rel in internal.get("releases", [])
             if rel.get("name") in (version,) or rel.get("name", "").endswith(f"({version})")
             for c in rel.get("versionCodes", [])]
    if not codes:
        call("DELETE", f"/edits/{edit}")
        fail(f"no internal-track release for {version} (internal has {names}): "
             f"tag mainnet-v{version} first")
    release = {
        "name": version,
        "versionCodes": codes,
        "inAppUpdatePriority": priority,
        "releaseNotes": [{"language": "en-US", "text": notes}],
    }
    if percent == 100:
        release["status"] = "completed"
    else:
        release.update(status="inProgress", userFraction=percent / 100)
    if dry:
        production = call("GET", f"/edits/{edit}/tracks/production")
        call("DELETE", f"/edits/{edit}")
        live = [(r.get("name"), r.get("status")) for r in production.get("releases", [])]
        print(f"dry run: would promote {version} (codes {', '.join(codes)}), {cls}, priority "
              f"{priority}, {percent:g}%; production now {live}; edit discarded")
        return
    call("PUT", f"/edits/{edit}/tracks/production",
         json={"track": "production", "releases": [release]})
    call("POST", f"/edits/{edit}:commit")
    print(f"production: {version} (codes {', '.join(codes)}), {cls}, priority {priority}, "
          f"{percent:g}% (sent for review; with managed publishing, publish it in the Console)")


if __name__ == "__main__":
    main()

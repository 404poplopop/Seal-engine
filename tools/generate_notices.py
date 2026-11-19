#!/usr/bin/env python3
"""Refresh the generated crate list in THIRD_PARTY_NOTICES.md from cargo metadata."""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NOTICES = ROOT / "THIRD_PARTY_NOTICES.md"
START = "<!-- notices:generated -->"
END = "<!-- notices:end -->"


def crate_lines():
    raw = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        text=True,
    )
    meta = json.loads(raw)
    rows = []
    for pkg in meta["packages"]:
        if pkg["name"].startswith("seal-") or pkg["name"] == "seal":
            continue
        license_name = pkg.get("license") or "license not declared"
        rows.append(f"- `{pkg['name']}` {pkg['version']} — {license_name}")
    rows.sort()
    return "\n".join(rows) + "\n"


def render(body):
    block = START + "\n" + body + END + "\n"
    text = NOTICES.read_text()
    if START in text and END in text:
        pre, rest = text.split(START, 1)
        _, post = rest.split(END, 1)
        post = post.lstrip("\n")
        return pre + block + post
    return text.rstrip() + "\n\n" + block


def main():
    check = "--check" in sys.argv
    updated = render(crate_lines())
    if check:
        if updated != NOTICES.read_text():
            print("THIRD_PARTY_NOTICES.md is stale. Run tools/generate_notices.py", file=sys.stderr)
            return 1
        print("third-party notices: current")
        return 0
    NOTICES.write_text(updated)
    print(f"wrote {NOTICES}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

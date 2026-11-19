#!/usr/bin/env python3
"""Write HTML and LCOV coverage with cargo llvm-cov.

Install the tool once:

    cargo install cargo-llvm-cov --locked

Then, from the repository root:

    python3 tools/coverage.py

HTML is written to target/coverage and LCOV to target/coverage/lcov.info.
Pass --text FILE to also write the plain summary. When cargo llvm-cov is
not installed the script prints that and exits 0, unless --require is set.
"""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path


def installed() -> bool:
    if shutil.which("cargo") is None:
        return False
    probe = subprocess.run(
        ["cargo", "llvm-cov", "--version"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return probe.returncode == 0


def run(args: list[str]) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.check_call(args)


def main() -> int:
    parser = argparse.ArgumentParser(description="Write HTML and LCOV coverage reports.")
    parser.add_argument("--html", default="target/coverage", help="HTML report directory")
    parser.add_argument("--lcov", default="target/coverage/lcov.info", help="LCOV output file")
    parser.add_argument("--text", default="", help="plain summary file")
    parser.add_argument(
        "--require",
        action="store_true",
        help="exit 2 when cargo llvm-cov is not installed",
    )
    options = parser.parse_args()
    if not installed():
        print(
            "cargo llvm-cov is not installed. Install it with: cargo install cargo-llvm-cov --locked",
            file=sys.stderr,
        )
        return 2 if options.require else 0
    html = Path(options.html)
    lcov = Path(options.lcov)
    html.mkdir(parents=True, exist_ok=True)
    lcov.parent.mkdir(parents=True, exist_ok=True)
    run(["cargo", "llvm-cov", "--workspace", "--html", "--output-dir", str(html)])
    run(["cargo", "llvm-cov", "--workspace", "--lcov", "--output-path", str(lcov)])
    if options.text:
        text = Path(options.text)
        if text.parent != Path(""):
            text.parent.mkdir(parents=True, exist_ok=True)
        run(["cargo", "llvm-cov", "--workspace", "--text", "--output-path", str(text)])
    print(f"html {html}")
    print(f"lcov {lcov}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Turn `cargo test` text output into a JUnit XML report.

    cargo test --workspace -- --nocapture > test-output.txt
    python3 tools/cargo_junit.py test-output.txt test-report.xml

Reads stdin when the input path is `-`. A self-check runs with `--self-test`.
"""

import sys
import xml.etree.ElementTree as ET


def escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def parse_output(text: str) -> list[dict]:
    cases: list[dict] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        if line.startswith("test ") and " ... " in line:
            name, status = line[len("test ") :].split(" ... ", 1)
            name = name.strip()
            status = status.strip()
            message = ""
            if status not in ("ok", "FAILED", "ignored"):
                index += 1
                continue
            if status == "FAILED":
                message = failure_message(lines, name)
            cases.append({"name": name, "status": status, "message": message})
        index += 1
    return cases


def failure_message(lines: list[str], name: str) -> str:
    header = f"---- {name} stdout ----"
    for index, line in enumerate(lines):
        if line.strip() == header:
            body: list[str] = []
            for follow in lines[index + 1 :]:
                if follow.startswith("---- ") and follow.rstrip().endswith(" stdout ----"):
                    break
                if follow.startswith("failures:"):
                    break
                body.append(follow)
            text = "\n".join(body).strip()
            if text:
                return text
    return "failed"


def to_junit(cases: list[dict]) -> str:
    failures = sum(1 for case in cases if case["status"] == "FAILED")
    errors = 0
    root = ET.Element("testsuites")
    suite = ET.SubElement(
        root,
        "testsuite",
        {
            "name": "cargo-test",
            "tests": str(len(cases)),
            "failures": str(failures),
            "errors": str(errors),
        },
    )
    for case in cases:
        node = ET.SubElement(suite, "testcase", {"name": case["name"], "classname": "cargo"})
        if case["status"] == "FAILED":
            fail = ET.SubElement(node, "failure", {"message": case["message"]})
            fail.text = case["message"]
        elif case["status"] == "ignored":
            ET.SubElement(node, "skipped")
    ET.indent(root, space="  ")
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(root, encoding="unicode") + "\n"


SAMPLE = """\
running 3 tests
test alpha::holds ... ok
test beta::misses ... FAILED
test gamma::later ... ignored

failures:

---- beta::misses stdout ----
left: 1
right: 2

failures:
    beta::misses

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out
"""


def self_test() -> int:
    cases = parse_output(SAMPLE)
    names = [case["name"] for case in cases]
    if names != ["alpha::holds", "beta::misses", "gamma::later"]:
        print("names", names, file=sys.stderr)
        return 1
    failed = next(case for case in cases if case["status"] == "FAILED")
    if "left: 1" not in failed["message"]:
        print(failed, file=sys.stderr)
        return 1
    xml = to_junit(cases)
    if "alpha::holds" not in xml or "beta::misses" not in xml or "<failure" not in xml:
        print(xml, file=sys.stderr)
        return 1
    ET.fromstring(xml)
    print("cargo_junit self-test ok")
    return 0


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        return self_test()
    if len(sys.argv) != 3:
        print("usage: cargo_junit.py INPUT OUTPUT", file=sys.stderr)
        return 2
    source = sys.stdin.read() if sys.argv[1] == "-" else open(sys.argv[1], encoding="utf-8").read()
    xml = to_junit(parse_output(source))
    with open(sys.argv[2], "w", encoding="utf-8") as handle:
        handle.write(xml)
    print(f"wrote {sys.argv[2]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

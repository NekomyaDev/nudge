#!/usr/bin/env python3
"""NTF v1 conformance runner.

Runs every case in conformance/cases/ through a reference NTF validator and
checks that the verdict matches expected.json. The default reference is the
nudge compiler's `nudgec trace-check` (the validator the traces were frozen
against); other implementations can be exercised with --cmd, which must
accept a trace path and exit 0 for valid traces / non-zero with error text
on stderr+stdout otherwise.

Usage:
    python3 conformance/run.py                 # nudgec on PATH
    python3 conformance/run.py --cmd my-validator
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
CASES = HERE / "cases"


def load_cases():
    for d in sorted(CASES.iterdir()):
        exp = json.loads((d / "expected.json").read_text())
        yield d.name, (d / "trace.jsonl").read_text(), exp


def run_validator(cmd: list, trace_text: str, tmp: pathlib.Path):
    tmp.write_text(trace_text)
    proc = subprocess.run(cmd + [str(tmp)], capture_output=True, text=True)
    out = (proc.stdout + proc.stderr).strip()
    return proc.returncode, out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cmd", nargs="+", default=None,
                    help="validator command (default: nudgec trace-check)")
    args = ap.parse_args()
    cmd = args.cmd or ([shutil.which("nudgec"), "trace-check"]
                       if shutil.which("nudgec") else None)
    if cmd is None:
        print("nudgec not on PATH; pass --cmd <validator>", file=sys.stderr)
        return 2

    passed = failed = 0
    tmp = HERE / ".run.trace.jsonl"
    try:
        for name, text, exp in load_cases():
            code, out = run_validator(cmd, text, tmp)
            errors = [l for l in out.splitlines() if l.strip()]
            if exp["valid"]:
                ok = code == 0
                why = "" if ok else f"expected valid, got exit {code}: {out}"
            else:
                ok = code != 0 and all(
                    any(sub in l for l in errors) for sub in exp["errors_contain"]
                )
                why = "" if ok else (
                    f"expected errors {exp['errors_contain']}, "
                    f"exit {code}, got: {out or '(no output)'}")
            if ok:
                passed += 1
            else:
                failed += 1
                print(f"FAIL {name}: {why}")
    finally:
        tmp.unlink(missing_ok=True)

    print(f"{passed} passed, {failed} failed ({passed + failed} cases)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

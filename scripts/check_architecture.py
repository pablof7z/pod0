#!/usr/bin/env python3
"""Run Pod0's fast local/CI architecture ratchets."""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


def run(script: Path, *arguments: str) -> int:
    command = [sys.executable, str(script), *arguments]
    print(f"\n$ {' '.join(command)}", flush=True)
    return subprocess.run(command, check=False).returncode


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="also run negative fixtures for every checker",
    )
    args = parser.parse_args()
    scripts = Path(__file__).resolve().parent
    checks = [
        (scripts / "check_architecture_ownership.py", False),
        (scripts / "check_listening_single_writer.py", True),
        (scripts / "check_ui_storage_boundary.py", True),
        (scripts / "check_file_lengths.py", True),
        (scripts / "check_nmp_domain_boundary.py", True),
        (scripts / "check_rust_dependency_policy.py", True),
        (scripts / "check_rust_facade_boundary.py", True),
        (scripts / "check_rust_schema_policy.py", True),
    ]
    failed = False
    for script, has_self_test in checks:
        if args.self_test and has_self_test:
            failed = run(script, "--self-test") != 0 or failed
        failed = run(script) != 0 or failed
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Create a content-free product-proof cohort report from local exports."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from scripts.product_proof_cohort import analyze  # noqa: E402
from scripts.product_proof_export import (  # noqa: E402
    CohortInputError,
    reject_duplicate_keys,
)


def input_files(paths: list[str]) -> list[Path]:
    files: set[Path] = set()
    for raw_path in paths:
        path = Path(raw_path).expanduser()
        if path.is_dir():
            files.update(item for item in path.rglob("*.json") if item.is_file())
        elif path.is_file():
            files.add(path)
        else:
            raise CohortInputError(f"input does not exist: {raw_path}")
    if not files:
        raise CohortInputError("no JSON export files were found")
    return sorted(files)


def read_archive(path: Path) -> Any:
    try:
        return json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=reject_duplicate_keys,
        )
    except CohortInputError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CohortInputError(f"{path}: invalid JSON export") from error


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Aggregate consented Pod0 product-signal exports."
    )
    parser.add_argument("exports", nargs="+", help="JSON export files or directories")
    parser.add_argument("--output", help="write the aggregate JSON report to this path")
    args = parser.parse_args()
    try:
        files = input_files(args.exports)
        report = analyze([(str(path), read_archive(path)) for path in files])
        encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.output:
            Path(args.output).write_text(encoded, encoding="utf-8")
        else:
            print(encoded, end="")
    except CohortInputError as error:
        print(f"product-proof evaluation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

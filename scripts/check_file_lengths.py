#!/usr/bin/env python3
"""Enforce the 500-line hard limit and ratchet existing soft-limit debt."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def line_count(path: Path) -> int:
    return len(path.read_text(encoding="utf-8").splitlines())


def scanned_files(root: Path, policy: dict[str, object]) -> list[Path]:
    extensions = set(policy["extensions"])
    files: set[Path] = set()
    for relative in policy["scan_roots"]:
        directory = root / relative
        if directory.exists():
            files.update(
                path for path in directory.rglob("*")
                if path.is_file() and path.suffix in extensions
            )
    for relative in policy["scan_files"]:
        path = root / relative
        if path.exists() and path.suffix in extensions:
            files.add(path)
    return sorted(files)


def evaluate(
    counts: dict[str, int],
    baseline: dict[str, int],
    soft_limit: int,
    hard_limit: int,
) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    reports: list[str] = []
    for path, count in sorted(counts.items()):
        allowed = baseline.get(path)
        if count > hard_limit:
            errors.append(f"{path}: {count} lines exceeds hard limit {hard_limit}")
            continue
        if count < soft_limit:
            if allowed is not None:
                errors.append(
                    f"{path}: now {count} lines; remove stale soft baseline {allowed}"
                )
            continue
        if allowed is None:
            errors.append(
                f"{path}: new soft-limit debt at {count} lines; split below {soft_limit}"
            )
        elif count > allowed:
            errors.append(
                f"{path}: grew from soft baseline {allowed} to {count}; split or ratchet down"
            )
        elif count < allowed:
            errors.append(
                f"{path}: decreased from {allowed} to {count}; ratchet baseline down in this change"
            )
        else:
            reports.append(f"{path}: {count} lines (existing soft-limit debt)")

    for path in sorted(set(baseline) - set(counts)):
        errors.append(f"{path}: stale soft baseline for missing/unscanned file")
    return errors, reports


def validate(root: Path, policy_path: Path) -> tuple[list[str], list[str]]:
    policy = json.loads(policy_path.read_text(encoding="utf-8"))
    counts = {
        path.relative_to(root).as_posix(): line_count(path)
        for path in scanned_files(root, policy)
    }
    return evaluate(
        counts,
        policy["soft_baseline"],
        policy["soft_limit"],
        policy["hard_limit"],
    )


def self_test() -> None:
    baseline = {"existing.swift": 350}
    errors, reports = evaluate(
        {"existing.swift": 350}, baseline, soft_limit=300, hard_limit=500
    )
    assert not errors and reports
    errors, _ = evaluate(
        {"existing.swift": 351}, baseline, soft_limit=300, hard_limit=500
    )
    assert any("grew" in error for error in errors)
    errors, _ = evaluate(
        {"new.swift": 300}, {}, soft_limit=300, hard_limit=500
    )
    assert any("new soft-limit debt" in error for error in errors)
    errors, _ = evaluate(
        {"huge.swift": 501}, {}, soft_limit=300, hard_limit=500
    )
    assert any("hard limit" in error for error in errors)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--policy",
        default="docs/architecture/file-length-baseline.json",
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("File-length negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    errors, reports = validate(root, root / args.policy)
    for report in reports:
        print(f"- {report}")
    if errors:
        print("File-length policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"File-length policy passed; {len(reports)} soft-limit files reported")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

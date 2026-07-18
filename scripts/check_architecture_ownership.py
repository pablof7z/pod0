#!/usr/bin/env python3
"""Validate that every production Swift file has one explicit owner."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
import sys


REQUIRED_FIELDS = {
    "id",
    "classification",
    "current_owner",
    "target_owner",
    "reason",
    "boundary",
    "persisted_state",
    "priority",
    "includes",
}
MIGRATING = {
    "shared_rust_now",
    "temporary_swift",
    "undecided_pending_investigation",
}


def path_matches(path: str, selector: str) -> bool:
    """Selectors ending in `/` or an incomplete filename are prefixes."""
    return path.startswith(selector) if selector.endswith("/") else (
        path == selector or path.startswith(selector) and not selector.endswith(".swift")
    )


def entry_matches(path: str, entry: dict[str, object]) -> bool:
    includes = entry["includes"]
    excludes = entry.get("excludes", [])
    return any(path_matches(path, item) for item in includes) and not any(
        path_matches(path, item) for item in excludes
    )


def production_swift_files(root: Path, roots: list[str]) -> list[str]:
    files: list[str] = []
    for relative_root in roots:
        directory = root / relative_root
        files.extend(
            path.relative_to(root).as_posix()
            for path in directory.rglob("*.swift")
            if path.is_file()
        )
    return sorted(set(files))


def validate_inventory(root: Path, inventory_path: Path) -> tuple[list[str], Counter[str]]:
    data = json.loads(inventory_path.read_text(encoding="utf-8"))
    errors: list[str] = []
    allowed = set(data["classifications"])
    entries = data["entries"]
    identifiers: set[str] = set()

    for index, entry in enumerate(entries):
        missing = REQUIRED_FIELDS - set(entry)
        if missing:
            errors.append(f"entry[{index}] missing fields: {sorted(missing)}")
            continue
        identifier = entry["id"]
        if identifier in identifiers:
            errors.append(f"duplicate entry id: {identifier}")
        identifiers.add(identifier)
        classification = entry["classification"]
        if classification not in allowed:
            errors.append(f"{identifier}: unsupported classification {classification}")
        if classification in MIGRATING and not entry.get("migration_issues"):
            errors.append(f"{identifier}: migrating owner has no migration issue")
        if classification in MIGRATING and not entry.get("deletion_target"):
            errors.append(f"{identifier}: migrating owner has no deletion target")

    files = production_swift_files(root, data["production_roots"])
    counts: Counter[str] = Counter()
    matched_entries: Counter[str] = Counter()
    for path in files:
        matches = [entry for entry in entries if entry_matches(path, entry)]
        if not matches:
            errors.append(f"uncovered production file: {path}")
            continue
        if len(matches) > 1:
            ids = ", ".join(entry["id"] for entry in matches)
            errors.append(f"ambiguous production file: {path} -> {ids}")
            continue
        entry = matches[0]
        counts[entry["classification"]] += 1
        matched_entries[entry["id"]] += 1

    for entry in entries:
        if matched_entries[entry["id"]] == 0:
            errors.append(f"stale inventory entry matches no file: {entry['id']}")

    counts["total"] = len(files)
    return errors, counts


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--inventory",
        default="docs/architecture/ownership.json",
        help="inventory path relative to repository root",
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
        help="repository root",
    )
    args = parser.parse_args()

    root = Path(args.root).resolve()
    inventory = root / args.inventory
    errors, counts = validate_inventory(root, inventory)
    if errors:
        print("Architecture ownership inventory failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"Covered production Swift files: {counts.pop('total')}")
    for classification, count in sorted(counts.items()):
        print(f"- {classification}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

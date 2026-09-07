#!/usr/bin/env python3
"""Validate source and behavioral ownership across every product language."""

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


def production_files(root: Path, sources: dict[str, list[str]]) -> list[str]:
    files: list[str] = []
    for extension, roots in sources.items():
        for relative_root in roots:
            directory = root / relative_root
            files.extend(
                path.relative_to(root).as_posix()
                for path in directory.rglob(f"*{extension}")
                if path.is_file()
            )
    return sorted(set(files))


def matching_entries(path: str, entries: list[dict[str, object]]) -> list[dict[str, object]]:
    """Return the most-specific owner; equal specificity remains ambiguous."""
    candidates = [entry for entry in entries if entry_matches(path, entry)]
    if not candidates:
        return []
    specificity = {
        id(entry): max(
            len(selector)
            for selector in entry["includes"]
            if path_matches(path, selector)
        )
        for entry in candidates
    }
    maximum = max(specificity.values())
    return [entry for entry in candidates if specificity[id(entry)] == maximum]


def validate_behavioral_inventories(
    root: Path, coverage: dict[str, object], entries: list[dict[str, object]]
) -> tuple[list[str], Counter[str]]:
    errors: list[str] = []
    counts: Counter[str] = Counter()
    facts: dict[str, str] = {}
    for manifest in coverage.get("behavioral_inventories", []):
        relative_path = manifest["path"]
        try:
            items = json.loads(
                (root / relative_path).read_text(encoding="utf-8")
            )["items"]
        except (OSError, KeyError, json.JSONDecodeError) as error:
            errors.append(f"{relative_path}: {error}")
            continue
        identity_fields = manifest["identity_fields"]
        owner_fields = manifest["owner_fields"]
        fact_fields = manifest.get("fact_fields", [])
        fact_owner_field = manifest.get("fact_owner_field")
        seen: set[tuple[object, ...]] = set()
        for item in items:
            identity = tuple(item.get(field) for field in identity_fields)
            if any(value in (None, "") for value in identity):
                errors.append(f"{relative_path}: incomplete identity {identity}")
            elif identity in seen:
                errors.append(f"{relative_path}: duplicate identity {identity}")
            seen.add(identity)
            owners = [item[field] for field in owner_fields if item.get(field)]
            if len(owners) != 1:
                errors.append(
                    f"{relative_path}:{identity}: expected one owner, found {owners}"
                )
                continue
            counts[item.get(manifest.get("kind_field"), manifest["kind"])] += 1
            source_path = item.get("path")
            if source_path and len(matching_entries(source_path, entries)) != 1:
                errors.append(
                    f"{relative_path}:{identity}: source has no exact file owner"
                )
            for field in fact_fields:
                fact = item.get(field)
                if not fact or fact == "none_rejected_at_boundary":
                    continue
                fact_owner = item.get(fact_owner_field) if fact_owner_field else owners[0]
                if not fact_owner:
                    errors.append(f"{relative_path}:{identity}: fact has no owner")
                    continue
                previous = facts.setdefault(fact, fact_owner)
                if previous != fact_owner:
                    errors.append(
                        f"fact {fact} has multiple owners: {previous}, {fact_owner}"
                    )
    for pattern in coverage.get("derived_behavior_patterns", []):
        paths: set[str] = set()
        for relative_root in pattern["roots"]:
            for path in (root / relative_root).rglob("*.rs"):
                if any(token in path.name for token in pattern["filename_contains"]):
                    paths.add(path.relative_to(root).as_posix())
        for path in paths:
            if len(matching_entries(path, entries)) != 1:
                errors.append(f"{pattern['kind']}:{path}: no exact file owner")
        counts[pattern["kind"]] = len(paths)
    counts["fact"] = len(facts)
    return errors, counts


def validate_inventory(root: Path, inventory_path: Path) -> tuple[list[str], Counter[str]]:
    data = json.loads(inventory_path.read_text(encoding="utf-8"))
    coverage = json.loads(
        (root / data["coverage_manifest"]).read_text(encoding="utf-8")
    )
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

    sources = coverage["production_sources"]
    files = production_files(root, sources)
    counts: Counter[str] = Counter()
    languages: Counter[str] = Counter()
    matched_entries: Counter[str] = Counter()
    for path in files:
        matches = matching_entries(path, entries)
        if not matches:
            errors.append(f"uncovered production file: {path}")
            continue
        if len(matches) > 1:
            ids = ", ".join(entry["id"] for entry in matches)
            errors.append(f"ambiguous production file: {path} -> {ids}")
            continue
        entry = matches[0]
        counts[entry["classification"]] += 1
        languages[Path(path).suffix.lstrip(".")] += 1
        matched_entries[entry["id"]] += 1

    for entry in entries:
        if matched_entries[entry["id"]] == 0:
            errors.append(f"stale inventory entry matches no file: {entry['id']}")

    behavior_errors, behavior_counts = validate_behavioral_inventories(
        root, coverage, entries
    )
    errors.extend(behavior_errors)
    counts.update({f"behavior:{key}": value for key, value in behavior_counts.items()})
    counts.update({f"language:{key}": value for key, value in languages.items()})
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

    print(f"Covered production source files: {counts.pop('total')}")
    for classification, count in sorted(counts.items()):
        print(f"- {classification}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

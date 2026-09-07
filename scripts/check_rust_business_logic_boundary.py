#!/usr/bin/env python3
"""Freeze native business-logic exceptions until Rust cutovers delete them."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

from check_architecture_ownership import matching_entries, production_files
from native_business_logic_scan import (
    FROZEN_EXCEPTION_PATHS,
    semantic_violations,
)


DECLARATION = re.compile(
    r"(?m)^(?:@\w+(?:\([^\n]*\))?\s*)*"
    r"(?:(?:public|internal|private|fileprivate|final|indirect|nonisolated)\s+)*"
    r"(?:class|struct|enum|actor|protocol|extension|typealias)\s+"
    r"([A-Za-z_][A-Za-z0-9_.+]*)"
)
ALLOWED_CHILD_ISSUES = {210, 213, 214, 215, 216, 217, 218}
PROHIBITED_TEMPLATE_TEXT = (
    "Temporary Swift behind a migration-safe boundary",
    "Temporary Swift without an issue",
)
def symbols(path: Path) -> list[str]:
    result: list[str] = []
    for match in DECLARATION.finditer(path.read_text(encoding="utf-8")):
        if match.group(1) not in result:
            result.append(match.group(1))
    return result


def native_files(
    root: Path, ownership: dict[str, object], coverage: dict[str, object]
) -> tuple[list[str], dict[str, dict[str, object]], list[str]]:
    sources = {
        extension: roots
        for extension, roots in coverage["production_sources"].items()
        if extension in {".swift", ".kt"}
    }
    files = production_files(root, sources)
    owners: dict[str, dict[str, object]] = {}
    errors: list[str] = []
    for path in files:
        matches = matching_entries(path, ownership["entries"])
        if len(matches) != 1:
            errors.append(f"native semantic scan has no exact owner: {path}")
            continue
        owners[path] = matches[0]
    return files, owners, errors


def temporary_files(
    files: list[str], owners: dict[str, dict[str, object]]
) -> tuple[dict[str, str], list[str]]:
    result: dict[str, str] = {}
    errors: list[str] = []
    for path in files:
        entry = owners.get(path)
        if entry is None:
            continue
        classification = entry["classification"]
        if classification == "undecided_pending_investigation":
            errors.append(f"undecided production owner is forbidden: {path}")
        if classification == "temporary_swift":
            result[path] = entry["id"]
    return result, errors


def validate(root: Path) -> list[str]:
    architecture = root / "docs/architecture"
    try:
        ownership = json.loads(
            (architecture / "ownership.json").read_text(encoding="utf-8")
        )
        policy = json.loads(
            (architecture / "rust-business-logic-exceptions.json").read_text(
                encoding="utf-8"
            )
        )
        coverage = json.loads(
            (root / ownership["coverage_manifest"]).read_text(encoding="utf-8")
        )
    except (OSError, json.JSONDecodeError) as error:
        return [str(error)]
    files, owners, errors = native_files(root, ownership, coverage)
    actual, temporary_errors = temporary_files(files, owners)
    errors.extend(temporary_errors)
    rows = policy.get("exceptions", [])
    registered = {row.get("path"): row for row in rows}
    if len(registered) != len(rows):
        errors.append("duplicate Rust business-logic exception path")
    for path in sorted(set(registered) - FROZEN_EXCEPTION_PATHS):
        errors.append(f"new native business-logic exception is forbidden: {path}")
    for path in sorted(set(actual) - set(registered)):
        errors.append(f"temporary Swift file missing exact exception: {path}")
    for path in sorted(set(registered) - set(actual)):
        errors.append(f"stale Rust business-logic exception: {path}")
    maximum = policy.get("maximum_exception_files")
    if not isinstance(maximum, int):
        errors.append("native business-logic exception ceiling must be an integer")
    elif len(rows) != maximum:
        errors.append(
            "native business-logic exception ceiling must equal the exact "
            f"current set: {len(rows)} != {maximum}"
        )
    allowed_roles = set(policy.get("allowed_roles", []))
    for path, row in registered.items():
        if actual.get(path) != row.get("ownership_id"):
            errors.append(f"{path}: ownership id drift")
        if row.get("child_issue") not in ALLOWED_CHILD_ISSUES:
            errors.append(f"{path}: invalid #204 cutover issue")
        if row.get("allowed_role") not in allowed_roles:
            errors.append(f"{path}: unsupported exception role")
        if not row.get("deletion_condition"):
            errors.append(f"{path}: deletion condition is required")
        source = root / path
        if source.is_file() and symbols(source) != row.get("symbols"):
            errors.append(
                f"{path}: declarations changed; review the exception exactly"
            )

    template = (root / ".github/pull_request_template.md").read_text(
        encoding="utf-8"
    )
    for phrase in PROHIBITED_TEMPLATE_TEXT:
        if phrase in template:
            errors.append(f"PR template still permits forbidden policy: {phrase}")

    for relative_path in files:
        owner = owners.get(relative_path)
        if owner is None or owner["classification"] in {
            "generated_binding",
            "delete_without_replacement",
        }:
            continue
        if relative_path in registered:
            continue
        for rule_id, line, description in semantic_violations(root / relative_path):
            errors.append(
                f"{relative_path}:{line}: {description} [{rule_id}] outside exact exception"
            )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root", default=str(Path(__file__).resolve().parents[1])
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        from rust_business_logic_boundary_fixtures import run_self_test

        return run_self_test(validate)
    root = Path(args.root).resolve()
    errors = validate(root)
    if errors:
        print("Rust business-logic ownership check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    ownership = json.loads(
        (root / "docs/architecture/ownership.json").read_text(encoding="utf-8")
    )
    coverage = json.loads(
        (root / ownership["coverage_manifest"]).read_text(encoding="utf-8")
    )
    files, _, _ = native_files(root, ownership, coverage)
    print(
        f"Rust business-logic semantic scan covered {len(files)} production "
        "Swift/Kotlin files; exception set is exact and non-growing"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

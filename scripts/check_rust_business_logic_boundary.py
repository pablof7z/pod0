#!/usr/bin/env python3
"""Freeze native business-logic exceptions until Rust cutovers delete them."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import tempfile

from check_architecture_ownership import entry_matches, production_swift_files


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
CANONICAL_ACTIVITY_CONSTRUCTION = re.compile(
    r"\b(?:DomainEventEnvelope|ActivityFact|ActivityEventEnvelope)\s*\("
)


def symbols(path: Path) -> list[str]:
    result: list[str] = []
    for match in DECLARATION.finditer(path.read_text(encoding="utf-8")):
        if match.group(1) not in result:
            result.append(match.group(1))
    return result


def temporary_files(
    root: Path, ownership: dict[str, object]
) -> tuple[dict[str, str], list[str]]:
    result: dict[str, str] = {}
    errors: list[str] = []
    for path in production_swift_files(root, ownership["production_roots"]):
        matches = [
            entry for entry in ownership["entries"] if entry_matches(path, entry)
        ]
        if len(matches) != 1:
            continue
        entry = matches[0]
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
    except (OSError, json.JSONDecodeError) as error:
        return [str(error)]
    actual, errors = temporary_files(root, ownership)
    rows = policy.get("exceptions", [])
    registered = {row.get("path"): row for row in rows}
    if len(registered) != len(rows):
        errors.append("duplicate Rust business-logic exception path")
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

    for relative_root in ownership["production_roots"]:
        directory = root / relative_root
        for path in directory.rglob("*.swift"):
            if CANONICAL_ACTIVITY_CONSTRUCTION.search(
                path.read_text(encoding="utf-8")
            ):
                errors.append(
                    "canonical Rust activity construction found in Swift: "
                    f"{path.relative_to(root).as_posix()}"
                )
    return errors


def write_fixture(root: Path, extra_symbol: bool) -> None:
    source = root / "App/Sources/Legacy.swift"
    source.parent.mkdir(parents=True)
    source.write_text(
        "struct Legacy {}\n" + ("struct AddedPolicy {}\n" if extra_symbol else ""),
        encoding="utf-8",
    )
    architecture = root / "docs/architecture"
    architecture.mkdir(parents=True)
    ownership = {
        "production_roots": ["App/Sources"],
        "entries": [{
            "id": "legacy", "classification": "temporary_swift",
            "includes": ["App/Sources/Legacy.swift"],
        }],
    }
    policy = {
        "maximum_exception_files": 1,
        "allowed_roles": ["legacy_product_policy"],
        "exceptions": [{
            "ownership_id": "legacy",
            "path": "App/Sources/Legacy.swift",
            "symbols": ["Legacy"],
            "allowed_role": "legacy_product_policy",
            "child_issue": 213,
            "deletion_condition": "Delete in #213.",
        }],
    }
    (architecture / "ownership.json").write_text(
        json.dumps(ownership), encoding="utf-8"
    )
    (architecture / "rust-business-logic-exceptions.json").write_text(
        json.dumps(policy), encoding="utf-8"
    )
    github = root / ".github"
    github.mkdir()
    (github / "pull_request_template.md").write_text(
        "Rust business logic only.\n", encoding="utf-8"
    )


def run_self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_symbol=False)
        if validate(root):
            print("Rust business-logic valid fixture failed", file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_symbol=True)
        if not any("declarations changed" in item for item in validate(root)):
            print("Rust business-logic checker missed new policy", file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_symbol=False)
        policy_path = root / "docs/architecture/rust-business-logic-exceptions.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["maximum_exception_files"] = 2
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        if not any("ceiling must equal" in item for item in validate(root)):
            print("Rust business-logic checker permitted ceiling slack", file=sys.stderr)
            return 1
    print("Rust business-logic negative fixtures passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root", default=str(Path(__file__).resolve().parents[1])
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return run_self_test()
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Rust business-logic ownership check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Rust business-logic exception set is exact and non-growing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

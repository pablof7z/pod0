#!/usr/bin/env python3
"""Require sequential, immutable, explicitly versioned Rust SQL migrations."""

from __future__ import annotations

import hashlib
from pathlib import Path
import re
import sys


def evaluate(
    current_version: int,
    migrations: dict[str, bytes],
    locked_hashes: dict[str, str],
    fixture_supported_max: int,
) -> list[str]:
    errors: list[str] = []
    expected_names = []
    for version in range(1, current_version + 1):
        prefix = f"{version:04d}_"
        matches = [name for name in migrations if name.startswith(prefix)]
        if len(matches) != 1:
            errors.append(
                f"schema version {version} must have exactly one migration, found {matches}"
            )
        else:
            expected_names.append(matches[0])
    unexpected = sorted(set(migrations) - set(expected_names))
    if unexpected:
        errors.append(f"unversioned or future migrations found: {unexpected}")
    if set(locked_hashes) != set(migrations):
        errors.append("migration lock entries must exactly match migration files")
    for name, content in migrations.items():
        digest = hashlib.sha256(content).hexdigest()
        if locked_hashes.get(name) != digest:
            errors.append(f"{name}: content changed without an updated migration lock")
    if fixture_supported_max != current_version:
        errors.append(
            "cross-language schema fixture supported_max must match current schema version"
        )
    return errors


def parse_properties(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if not separator or not key or key in result:
            raise ValueError(f"invalid fixture line: {line!r}")
        result[key] = value
    return result


def validate(root: Path) -> list[str]:
    model = (
        root / "rust/crates/pod0-storage/src/model.rs"
    ).read_text(encoding="utf-8")
    match = re.search(r"CURRENT_SCHEMA_VERSION: u32 = (\d+);", model)
    if match is None:
        return ["CURRENT_SCHEMA_VERSION declaration is missing"]
    current_version = int(match.group(1))
    migration_root = root / "rust/schema/migrations"
    migrations = {
        path.name: path.read_bytes()
        for path in sorted(migration_root.glob("*.sql"))
    }
    locked_hashes = {}
    for line in (root / "rust/schema/migrations.lock").read_text().splitlines():
        digest, name = line.split()
        locked_hashes[name] = digest
    fixture = parse_properties(
        root / "Fixtures/CoreSchema/schema-status-v1.properties"
    )
    return evaluate(
        current_version,
        migrations,
        locked_hashes,
        int(fixture["supported_max"]),
    )


def self_test() -> None:
    migration = b"CREATE TABLE example(id INTEGER);\n"
    digest = hashlib.sha256(migration).hexdigest()
    assert not evaluate(1, {"0001_example.sql": migration}, {"0001_example.sql": digest}, 1)
    errors = evaluate(1, {"unversioned.sql": migration}, {}, 2)
    assert any("exactly one migration" in error for error in errors)
    assert any("supported_max" in error for error in errors)


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        print("Rust schema-policy negative fixtures passed")
        return 0
    errors = validate(Path(__file__).resolve().parents[1])
    if errors:
        print("Rust schema policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Rust schema policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

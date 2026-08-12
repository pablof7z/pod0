#!/usr/bin/env python3
"""Keep protocol machinery out of Pod0's product Rust workspace."""

from __future__ import annotations

from pathlib import Path
import re
import sys


UNIFFI_VERSION = "0.32.0"
RUSQLITE_VERSION = "0.39.0"


def manifest_dependencies(text: str) -> list[tuple[str, str]]:
    dependencies: list[tuple[str, str]] = []
    in_dependencies = False
    for raw_line in text.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip().strip("'").strip('"')
            in_dependencies = section.endswith("dependencies")
            continue
        if in_dependencies and "=" in line:
            name, specification = line.split("=", 1)
            dependencies.append((name.strip().strip('"'), specification.strip()))
    return dependencies


def dependency_errors(relative: str, name: str, specification: str) -> list[str]:
    errors: list[str] = []
    package_match = re.search(r'package\s*=\s*"([^"]+)"', specification)
    package = package_match.group(1) if package_match else name
    if package == "nmp" or package.startswith("nmp-"):
        errors.append(
            f"{relative}: mechanism/protocol crate dependency {package!r} is forbidden"
        )
    if "git" in specification and "rev" not in specification:
        errors.append(f"{relative}: Git dependency {name!r} must use an exact rev")
    return errors


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    rust = root / "rust"
    workspace_text = (rust / "Cargo.toml").read_text(encoding="utf-8")
    expected_uniffi = f'uniffi = {{ version = "={UNIFFI_VERSION}" }}'
    if expected_uniffi not in workspace_text:
        errors.append(f"workspace UniFFI dependency must equal {expected_uniffi!r}")
    expected_rusqlite = f'rusqlite = {{ version = "={RUSQLITE_VERSION}"'
    if expected_rusqlite not in workspace_text:
        errors.append(
            f"workspace rusqlite dependency must begin with {expected_rusqlite!r}"
        )

    lock = rust / "Cargo.lock"
    if not lock.exists():
        errors.append("rust/Cargo.lock must be committed")

    for manifest in sorted((rust / "crates").glob("*/Cargo.toml")):
        relative = manifest.relative_to(rust).as_posix()
        text = manifest.read_text(encoding="utf-8")
        for name, specification in manifest_dependencies(text):
            errors.extend(dependency_errors(relative, name, specification))
    return errors


def self_test() -> None:
    fixture = '[dependencies]\nnmp = { workspace = true }\n[package]\nname = "fixture"'
    assert manifest_dependencies(fixture) == [("nmp", "{ workspace = true }")]
    assert dependency_errors(
        "crates/pod0-facade/Cargo.toml", "nmp", "{ workspace = true }"
    ) == [
        "crates/pod0-facade/Cargo.toml: mechanism/protocol crate dependency "
        "'nmp' is forbidden"
    ]
    assert dependency_errors(
        "crates/pod0-facade/Cargo.toml", "nmp-store", "{ git = \"example\" }"
    ) == [
        "crates/pod0-facade/Cargo.toml: mechanism/protocol crate dependency "
        "'nmp-store' is forbidden",
        "crates/pod0-facade/Cargo.toml: Git dependency 'nmp-store' must use an exact rev",
    ]


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        print("Rust dependency-policy negative fixture passed")
        return 0
    root = Path(__file__).resolve().parents[1]
    errors = validate(root)
    if errors:
        print("Rust dependency policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Rust dependency policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Prevent the removed iOS SQLiteVec package and vector API from returning."""

from __future__ import annotations

from pathlib import Path
import sys


FORBIDDEN = (
    "CSQLiteVec",
    "jkrukowski/SQLiteVec",
    '.package(product: "SQLiteVec")',
    "sqlite3_vec",
    "USING vec0",
)

SCANNED_ROOTS = (
    "App/Sources",
    "AppTests/Sources",
    "Podcastr.xcodeproj/project.pbxproj",
    ".package.resolved",
)


def scanned_files(root: Path) -> list[Path]:
    files = [root / "Project.swift"]
    for relative in SCANNED_ROOTS:
        candidate = root / relative
        if candidate.is_file():
            files.append(candidate)
        elif candidate.is_dir():
            files.extend(path for path in candidate.rglob("*") if path.is_file())
    return files


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for path in scanned_files(root):
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        relative = path.relative_to(root).as_posix()
        for token in FORBIDDEN:
            if token in text:
                errors.append(f"{relative}: forbidden SQLiteVec reference {token!r}")

    module_map = root / "App/Support/CSQLite3/module.modulemap"
    shim = root / "App/Support/CSQLite3/sqlite3_shim.h"
    project = (root / "Project.swift").read_text(encoding="utf-8")
    if not module_map.exists() or 'link "sqlite3"' not in module_map.read_text():
        errors.append("CSQLite3 module must auto-link the Apple SQLite library")
    if not shim.exists() or "#include <sqlite3.h>" not in shim.read_text():
        errors.append("CSQLite3 shim must expose only the system sqlite3 header")
    if project.count('"OTHER_LDFLAGS": "$(inherited) -lsqlite3"') != 2:
        errors.append("app and test targets must explicitly link Apple libsqlite3")
    return errors


def self_test() -> None:
    fixture = 'import CSQLiteVec\nlet query = "USING vec0"'
    assert [token for token in FORBIDDEN if token in fixture] == [
        "CSQLiteVec",
        "USING vec0",
    ]


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        print("SQLiteVec-absence negative fixture passed")
        return 0
    root = Path(__file__).resolve().parents[1]
    errors = validate(root)
    if errors:
        print("SQLiteVec absence policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("SQLiteVec absence policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Reject every remaining name or text reference to the removed protocol."""

from __future__ import annotations

import os
from pathlib import Path
import re
import sys
import tempfile


TERMS = (
    b"no" + b"str",
    b"n" + b"mp",
    b"da" + b"mus",
    b"pri" + b"mal",
    b"blos" + b"som",
    b"bun" + b"ker://",
)
TOKEN_PATTERN = re.compile(
    b"(?<![a-z0-9])n" + b"(?:(?:pub|sec)(?![a-z0-9])|ip[-_ ]?[0-9]+)"
)
SKIP_DIRECTORIES = {".git", ".build", "target", "DerivedData"}
TEXT_SUFFIXES = {
    "", ".c", ".cc", ".cpp", ".h", ".json", ".kt", ".lock", ".md",
    ".plist", ".py", ".rs", ".sh", ".sql", ".swift", ".toml", ".txt",
    ".xcconfig", ".yml", ".yaml",
}


def findings(root: Path) -> list[str]:
    errors: list[str] = []
    for directory, names, files in os.walk(root):
        names[:] = sorted(name for name in names if name not in SKIP_DIRECTORIES)
        for name in names + sorted(files):
            relative = (Path(directory) / name).relative_to(root).as_posix()
            lowered_name = name.casefold().encode()
            if any(term in lowered_name for term in TERMS) or TOKEN_PATTERN.search(lowered_name):
                errors.append(f"{relative}: removed protocol appears in path")
        for name in sorted(files):
            path = Path(directory) / name
            if path.suffix.casefold() not in TEXT_SUFFIXES:
                continue
            try:
                lowered = path.read_bytes().lower()
            except OSError as error:
                errors.append(f"{path.relative_to(root)}: unreadable: {error}")
                continue
            if any(term in lowered for term in TERMS) or TOKEN_PATTERN.search(lowered):
                errors.append(f"{path.relative_to(root)}: removed protocol appears in content")
    return errors


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "clean.swift").write_text("let value = 1\n")
        assert findings(root) == []
        term = TERMS[0].decode()
        (root / "clean.swift").write_text(f"// {term}\n")
        assert findings(root)
        (root / "clean.swift").write_text("let value = 1\n")
        (root / f"forbidden-{term}.swift").write_text("let value = 1\n")
        assert findings(root)


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        print("Removed-protocol absence negative fixtures passed")
        return 0
    root = Path(__file__).resolve().parents[1]
    errors = findings(root)
    if errors:
        print("Removed-protocol absence check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Removed-protocol absence check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Prevent Pod0 application nouns from entering vendored/local generic NMP crates."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


APP_NOUN = re.compile(
    r"\b(pod0|podcast|episode|subscription|transcript|briefing|"
    r"playback_policy|download_policy|queue_entry)",
    re.IGNORECASE,
)


def generic_nmp_roots(root: Path) -> list[Path]:
    candidates: list[Path] = []
    crates = root / "rust/crates"
    if crates.exists():
        candidates.extend(
            path for path in crates.iterdir()
            if path.is_dir() and path.name.startswith("nmp-")
        )
    vendor = root / "rust/vendor"
    if vendor.exists():
        candidates.extend(
            path for path in vendor.iterdir()
            if path.is_dir() and path.name.startswith("nmp")
        )
    return sorted(candidates)


def scan_text(text: str) -> list[tuple[str, int]]:
    findings: list[tuple[str, int]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        for match in APP_NOUN.finditer(line):
            findings.append((match.group(0), line_number))
    return findings


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for generic_root in generic_nmp_roots(root):
        for path in generic_root.rglob("*"):
            if not path.is_file() or path.suffix not in {".rs", ".toml", ".udl"}:
                continue
            for noun, line in scan_text(path.read_text(encoding="utf-8")):
                relative = path.relative_to(root).as_posix()
                errors.append(f"{relative}:{line}: Pod0 noun '{noun}' in generic NMP")
    return errors


def self_test() -> None:
    assert scan_text("pub struct EpisodeQueue {}") == [("Episode", 1)]
    assert scan_text("pub struct RelayRouting {}") == []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("NMP domain-boundary negative fixture passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("NMP domain boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("NMP domain boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

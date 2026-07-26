#!/usr/bin/env python3
"""Ratchet synchronous shared-core I/O out of UI-reachable Swift code."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys


CALL = re.compile(
    r"\bfacade\.(?P<operation>"
    r"dispatch|snapshot|subscribe|unsubscribe|nextHostCancellations|"
    r"nextHostRequests|recordHostObservation|modelChapterCutover"
    r")\s*\(|\bPod0Facade\.open\s*\("
)
DECLARATION = re.compile(
    r"^\s*(?P<prefix>(?:@\w+(?:\([^)]*\))?\s+|"
    r"(?:nonisolated|private|fileprivate|internal|public|static|class|"
    r"mutating|nonmutating|override|convenience|required)\s+)*)"
    r"(?P<kind>func|init)\b\s*(?P<name>[A-Za-z_][A-Za-z0-9_]*)?"
)
ACTOR_DECLARATION = re.compile(
    r"^\s*(?:(?:private|fileprivate|internal|public|package|final)\s+)*actor\b"
)


def strip_comments(source: str) -> str:
    output: list[str] = []
    in_block = False
    index = 0
    while index < len(source):
        if in_block:
            if source.startswith("*/", index):
                in_block = False
                output.extend("  ")
                index += 2
            else:
                output.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if source.startswith("/*", index):
            in_block = True
            output.extend("  ")
            index += 2
            continue
        if source.startswith("//", index):
            while index < len(source) and source[index] != "\n":
                output.append(" ")
                index += 1
            continue
        output.append(source[index])
        index += 1
    return "".join(output)


def findings(relative: str, source: str) -> list[tuple[str, int]]:
    code = strip_comments(source)
    current_symbol = "<file>"
    current_nonisolated = False
    brace_depth = 0
    actor_body_depth: int | None = None
    results: list[tuple[str, int]] = []
    offset = 0
    for line_number, line in enumerate(code.splitlines(keepends=True), 1):
        if actor_body_depth is None and ACTOR_DECLARATION.match(line):
            actor_body_depth = brace_depth + 1
        declaration = DECLARATION.match(line)
        if declaration:
            name = declaration.group("name") or "init"
            current_symbol = name
            current_nonisolated = "nonisolated" in declaration.group("prefix")
        if not current_nonisolated and actor_body_depth is None:
            for match in CALL.finditer(line):
                operation = match.group("operation") or "open"
                results.append((f"{relative}|{current_symbol}|{operation}", line_number))
        brace_depth += line.count("{") - line.count("}")
        if actor_body_depth is not None and brace_depth < actor_body_depth:
            actor_body_depth = None
        offset += len(line)
    return results


def production_files(root: Path) -> list[Path]:
    core = root / "App/Sources/Core"
    candidates = list(core.glob("SharedLibraryClient*.swift"))
    candidates += list(core.glob("SharedAgentConversation*.swift"))
    candidates += list(core.glob("SharedLibraryBootstrap*.swift"))
    return sorted(set(path for path in candidates if path.is_file()))


def current_counts(root: Path) -> tuple[dict[str, int], dict[str, list[int]]]:
    counts: dict[str, int] = {}
    lines: dict[str, list[int]] = {}
    for path in production_files(root):
        relative = path.relative_to(root).as_posix()
        for key, line in findings(relative, path.read_text(encoding="utf-8")):
            counts[key] = counts.get(key, 0) + 1
            lines.setdefault(key, []).append(line)
    return counts, lines


def validate(root: Path, policy_path: Path) -> list[str]:
    policy = json.loads(policy_path.read_text(encoding="utf-8"))
    baseline = policy["baseline"]
    counts, lines = current_counts(root)
    errors: list[str] = []
    for key in sorted(set(counts) | set(baseline)):
        current = counts.get(key, 0)
        allowed = baseline.get(key, 0)
        if current > allowed:
            location = ",".join(str(value) for value in lines.get(key, []))
            errors.append(
                f"{key}: {current} call(s), baseline {allowed}; lines {location}"
            )
        elif current < allowed:
            errors.append(
                f"{key}: improved from {allowed} to {current}; ratchet the baseline down"
            )
    return errors


def self_test() -> None:
    source = """
extension Client {
    func blocked() {
        facade.snapshot(request: request)
        // facade.dispatch(command: ignored)
    }
    nonisolated static func allowed(facade: Facade) {
        facade.snapshot(request: request)
    }
}
actor Worker {
    func allowed() {
        facade.dispatch(command: command)
    }
}
"""
    assert findings("Fixture.swift", source) == [
        ("Fixture.swift|blocked|snapshot", 4)
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--print-baseline", action="store_true")
    parser.add_argument(
        "--policy",
        default="docs/architecture/main-actor-core-io.json",
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Main-actor core-I/O negative fixture passed")
        return 0
    root = Path(args.root).resolve()
    if args.print_baseline:
        counts, _ = current_counts(root)
        print(json.dumps(counts, indent=2, sort_keys=True))
        return 0
    errors = validate(root, root / args.policy)
    if errors:
        print("Main-actor core-I/O boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Main-actor core-I/O boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

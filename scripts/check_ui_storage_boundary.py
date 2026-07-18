#!/usr/bin/env python3
"""Reject direct durable-store/runtime access from native presentation code."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys


def strip_swift_comments(source: str) -> str:
    """Remove line/block comments while preserving line numbers."""
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


def scan_source(source: str, rules: list[dict[str, str]]) -> list[tuple[str, int]]:
    code = strip_swift_comments(source)
    findings: list[tuple[str, int]] = []
    for rule in rules:
        pattern = re.compile(rule["regex"])
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            findings.append((rule["id"], line))
    return sorted(findings, key=lambda item: (item[1], item[0]))


def production_files(root: Path, scan_roots: list[str]) -> list[Path]:
    paths: list[Path] = []
    for relative in scan_roots:
        paths.extend((root / relative).rglob("*.swift"))
    return sorted(set(path for path in paths if path.is_file()))


def validate(root: Path, policy_path: Path) -> list[str]:
    data = json.loads(policy_path.read_text(encoding="utf-8"))
    rules = data["rules"]
    rule_ids = {rule["id"] for rule in rules}
    errors: list[str] = []
    exceptions: dict[tuple[str, str], dict[str, object]] = {}
    used_exceptions: set[tuple[str, str]] = set()

    for exception in data["exceptions"]:
        path = exception["file"]
        if not exception.get("issues") or not exception.get("deletion_target"):
            errors.append(f"exception lacks issue/deletion target: {path}")
        for rule_id in exception["rules"]:
            if rule_id not in rule_ids:
                errors.append(f"exception references unknown rule: {path} -> {rule_id}")
            key = (path, rule_id)
            if key in exceptions:
                errors.append(f"duplicate exception: {path} -> {rule_id}")
            exceptions[key] = exception

    for path in production_files(root, data["scan_roots"]):
        relative = path.relative_to(root).as_posix()
        source = path.read_text(encoding="utf-8")
        for rule_id, line in scan_source(source, rules):
            key = (relative, rule_id)
            if key in exceptions:
                used_exceptions.add(key)
            else:
                errors.append(f"{relative}:{line}: prohibited {rule_id}")

    for key in sorted(set(exceptions) - used_exceptions):
        errors.append(f"stale exception no longer matches code: {key[0]} -> {key[1]}")
    return errors


def self_test() -> None:
    rules = [
        {"id": "mutation", "regex": r"\.mutateState(?:\s*\(|\s*\{)"},
        {"id": "runtime", "regex": r"\bWorkflowRuntime\b"},
    ]
    source = """
    // WorkflowRuntime.shared is only a comment.
    let text = "not relevant"
    store.mutateState { $0.value = 1 }
    WorkflowRuntime.shared.wake()
    /* store.mutateState { ignored() } */
    """
    assert scan_source(source, rules) == [("mutation", 4), ("runtime", 5)]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--policy",
        default="docs/architecture/ui-storage-boundary.json",
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("UI/storage boundary negative fixture passed")
        return 0

    root = Path(args.root).resolve()
    errors = validate(root, root / args.policy)
    if errors:
        print("UI/storage boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("UI/storage boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

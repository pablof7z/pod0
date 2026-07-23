#!/usr/bin/env python3
"""Keep the Rust-owned agent-tool surface aligned with its authority matrix."""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/architecture/agent-tool-permissions.json"
RUST_TOOL_PATTERN = re.compile(
    r'\(\s*"([a-z][a-z0-9_]*)"\s*,\s*AgentToolName::', re.MULTILINE
)
RUST_TOOL_SOURCE = ROOT / "rust/crates/pod0-application/src/agent_tool_names.rs"
ALLOWED_CLASSES = {
    "read_only", "reversible_write", "external_side_effect",
    "destructive_write", "secret_bearing", "publication", "session_local",
}
ALLOWED_AUTHORITIES = {
    "none", "durable_turn_grant", "durable_scoped_grant", "one_shot_approval",
}
PRIVILEGED_CLASSES = {
    "external_side_effect", "destructive_write", "secret_bearing", "publication",
}
ALLOWED_EXECUTION = {
    "rust_commit", "rust_projection", "native_capability",
    "native_conversation_presentation",
    "native_capability_and_pod0_nmp_publication",
}


def source_tools(payload: dict) -> list[str]:
    tools: list[str] = []
    for relative in payload.get("source_files", []):
        tools.extend(RUST_TOOL_PATTERN.findall((ROOT / relative).read_text()))
    return tools


def validate(payload: dict, actual: list[str]) -> list[str]:
    errors: list[str] = []
    rows = payload.get("tools")
    if payload.get("schema_version") != 1 or not isinstance(rows, list):
        return ["matrix must have schema_version 1 and a tools array"]
    names = [row.get("tool") for row in rows if isinstance(row, dict)]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        errors.append(f"duplicate matrix tools: {', '.join(duplicates)}")
    missing = sorted(set(actual) - set(names))
    extra = sorted(set(names) - set(actual))
    if missing:
        errors.append(f"tools missing from matrix: {', '.join(missing)}")
    if extra:
        errors.append(f"matrix tools absent from the authoritative source: {', '.join(extra)}")
    if len(actual) != len(set(actual)):
        errors.append("the authoritative source declares duplicate canonical tool strings")
    rust_names = RUST_TOOL_PATTERN.findall(RUST_TOOL_SOURCE.read_text())
    if len(rust_names) != len(set(rust_names)):
        errors.append("Rust declares duplicate canonical tool strings")
    rust_missing = sorted(set(actual) - set(rust_names))
    rust_extra = sorted(set(rust_names) - set(actual))
    if rust_missing:
        errors.append(f"authority-matrix tools missing from Rust enum map: {', '.join(rust_missing)}")
    if rust_extra:
        errors.append(f"Rust enum-map tools absent from the authority matrix: {', '.join(rust_extra)}")

    for row in rows:
        name = row.get("tool", "<missing>")
        classes = row.get("classes")
        if not isinstance(classes, list) or not classes or len(classes) != len(set(classes)):
            errors.append(f"{name}: classes must be a non-empty unique array")
            continue
        unknown_classes = set(classes) - ALLOWED_CLASSES
        if unknown_classes:
            errors.append(f"{name}: unknown classes {sorted(unknown_classes)}")
        authority = row.get("authority")
        if authority not in ALLOWED_AUTHORITIES:
            errors.append(f"{name}: unknown authority {authority!r}")
        if set(classes) & PRIVILEGED_CLASSES and authority == "none":
            errors.append(f"{name}: privileged action must fail closed without durable authority")
        if row.get("decision_owner") != "pod0_rust":
            errors.append(f"{name}: durable decision owner must be pod0_rust")
        execution = row.get("execution")
        if execution not in ALLOWED_EXECUTION:
            errors.append(f"{name}: unknown execution boundary {execution!r}")
        if "publication" in classes and "pod0_nmp_publication" not in str(execution):
            errors.append(f"{name}: publication must route through pod0-nmp")

    if "relay" in json.dumps(payload).lower():
        errors.append("the app tool contract must not expose relay selection")
    if payload.get("decision_issue") != 132:
        errors.append("matrix must link decision issue #132")
    if payload.get("implementation_issues") != [133, 134, 135, 136, 137, 138]:
        errors.append("matrix must link implementation/cutover issues #133-#138")
    return errors


def self_test(payload: dict, actual: list[str]) -> list[str]:
    failures: list[str] = []
    cases = []
    missing = copy.deepcopy(payload)
    missing["tools"].pop()
    cases.append(("missing tool", missing))
    duplicate = copy.deepcopy(payload)
    duplicate["tools"].append(copy.deepcopy(duplicate["tools"][0]))
    cases.append(("duplicate tool", duplicate))
    unsafe = copy.deepcopy(payload)
    next(row for row in unsafe["tools"] if "external_side_effect" in row["classes"])["authority"] = "none"
    cases.append(("unauthorized external effect", unsafe))
    bypass = copy.deepcopy(payload)
    next(row for row in bypass["tools"] if "publication" in row["classes"])["execution"] = "native_capability"
    cases.append(("publication bypass", bypass))
    for label, fixture in cases:
        if not validate(fixture, actual):
            failures.append(f"self-test did not reject {label}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    payload = json.loads(MATRIX.read_text())
    actual = source_tools(payload)
    errors = self_test(payload, actual) if args.self_test else validate(payload, actual)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    label = "negative fixtures rejected" if args.self_test else f"{len(actual)} tools classified"
    print(f"Agent tool permission contract OK: {label}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

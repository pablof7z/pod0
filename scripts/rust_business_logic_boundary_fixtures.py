"""Negative fixtures for the native business-logic exception ratchet."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
from typing import Callable

from native_business_logic_scan import FROZEN_EXCEPTION_PATHS, RULES


Validator = Callable[[Path], list[str]]
LEGACY_PATH = "App/Sources/Features/Player/AutoSnip/AutoSnipController.swift"


def write_fixture(root: Path, extra_symbol: bool = False) -> None:
    source = root / LEGACY_PATH
    source.parent.mkdir(parents=True)
    source.write_text(
        "struct AutoSnipController {}\n"
        + ("struct AddedPolicy {}\n" if extra_symbol else ""),
        encoding="utf-8",
    )
    architecture = root / "docs/architecture"
    architecture.mkdir(parents=True)
    ownership = {
        "coverage_manifest": "docs/architecture/ownership-coverage.json",
        "production_roots": ["App/Sources"],
        "entries": [
            {
                "id": "legacy", "classification": "temporary_swift",
                "includes": [LEGACY_PATH],
            },
            {
                "id": "native", "classification": "native_by_design",
                "includes": ["App/Sources/Native.swift", "Android/Sources/"],
            },
        ],
    }
    coverage = {
        "production_sources": {
            ".swift": ["App/Sources"],
            ".kt": ["Android/Sources"],
        }
    }
    policy = {
        "maximum_exception_files": 1,
        "allowed_roles": ["legacy_product_policy"],
        "exceptions": [{
            "ownership_id": "legacy",
            "path": LEGACY_PATH,
            "symbols": ["AutoSnipController"],
            "allowed_role": "legacy_product_policy",
            "child_issue": 213,
            "deletion_condition": "Delete in #213.",
        }],
    }
    (architecture / "ownership.json").write_text(
        json.dumps(ownership), encoding="utf-8"
    )
    (architecture / "ownership-coverage.json").write_text(
        json.dumps(coverage), encoding="utf-8"
    )
    (root / "App/Sources/Native.swift").write_text(
        "struct NativeView { let help = \"ActivityFact(value)\" }\n"
        "// ExternalEffectDispatcher.shared.dispatch(effect)\n",
        encoding="utf-8",
    )
    kotlin = root / "Android/Sources/Native.kt"
    kotlin.parent.mkdir(parents=True)
    kotlin.write_text("class NativeView\n", encoding="utf-8")
    (architecture / "rust-business-logic-exceptions.json").write_text(
        json.dumps(policy), encoding="utf-8"
    )
    github = root / ".github"
    github.mkdir()
    (github / "pull_request_template.md").write_text(
        "Rust business logic only.\n", encoding="utf-8"
    )


def policy(root: Path) -> tuple[Path, dict[str, object]]:
    path = root / "docs/architecture/rust-business-logic-exceptions.json"
    return path, json.loads(path.read_text(encoding="utf-8"))


def run_self_test(validate: Validator) -> int:
    if LEGACY_PATH not in FROZEN_EXCEPTION_PATHS:
        print("Rust business-logic fixture path is not frozen", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root)
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
        write_fixture(root)
        path, current = policy(root)
        current["maximum_exception_files"] = 2
        path.write_text(json.dumps(current), encoding="utf-8")
        if not any("ceiling must equal" in item for item in validate(root)):
            print("Rust business-logic checker permitted ceiling slack", file=sys.stderr)
            return 1
    if not ratchet_fixtures_pass(validate):
        return 1
    if not semantic_fixtures_pass(validate):
        return 1
    print("Rust business-logic negative fixtures passed")
    return 0


def ratchet_fixtures_pass(validate: Validator) -> bool:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root)
        new_source = root / "App/Sources/NewPolicy.swift"
        new_source.write_text("struct NewPolicy {}\n", encoding="utf-8")
        ownership_path = root / "docs/architecture/ownership.json"
        ownership = json.loads(ownership_path.read_text(encoding="utf-8"))
        ownership["entries"].append({
            "id": "new", "classification": "temporary_swift",
            "includes": ["App/Sources/NewPolicy.swift"],
        })
        ownership_path.write_text(json.dumps(ownership), encoding="utf-8")
        path, current = policy(root)
        current["maximum_exception_files"] = 2
        current["exceptions"].append({
            "ownership_id": "new", "path": "App/Sources/NewPolicy.swift",
            "symbols": ["NewPolicy"], "allowed_role": "legacy_product_policy",
            "child_issue": 213, "deletion_condition": "Delete now.",
        })
        path.write_text(json.dumps(current), encoding="utf-8")
        if not any("new native business-logic exception" in item for item in validate(root)):
            print("Rust business-logic checker allowed a new exception", file=sys.stderr)
            return False
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root)
        root.joinpath(LEGACY_PATH).unlink()
        if not any("stale Rust business-logic exception" in item for item in validate(root)):
            print("Rust business-logic checker allowed a resolved stale row", file=sys.stderr)
            return False
        path, current = policy(root)
        current["maximum_exception_files"] = 0
        current["exceptions"] = []
        path.write_text(json.dumps(current), encoding="utf-8")
        if validate(root):
            print("Rust business-logic checker rejected empty exception set", file=sys.stderr)
            return False
    return True


def semantic_fixtures_pass(validate: Validator) -> bool:
    violation_sources = {
        "native_product_policy": "struct EpisodeBusinessPolicy {}\n",
        "direct_durable_write": "ProductStateStore.shared.save(value)\n",
        "semantic_fact_construction": "ActivityFact(value)\n",
        "direct_effect_dispatch": "ExternalEffectDispatcher.shared.dispatch(effect)\n",
        "native_default_fallback_retry": "fallbackProvider = provider\n",
        "in_memory_only_authorization": "authorizedEffects.append(effect)\n",
        "stale_observation_acceptance": "acceptStaleObservation(observation)\n",
        "restored_retired_writer": "let store = TranscriptStore.shared\n",
    }
    for index, (rule_id, source) in enumerate(violation_sources.items()):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_fixture(root)
            path = (
                root / "App/Sources/Native.swift"
                if index % 2 == 0
                else root / "Android/Sources/Native.kt"
            )
            path.write_text(source, encoding="utf-8")
            if not any(f"[{rule_id}]" in item for item in validate(root)):
                print(f"Rust business-logic checker missed {rule_id}", file=sys.stderr)
                return False
    if {item[0] for item in RULES} != set(violation_sources):
        print("Rust business-logic self-test does not cover every semantic rule", file=sys.stderr)
        return False
    return True

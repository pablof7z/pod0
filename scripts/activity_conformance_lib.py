"""Validation helpers for ADR-0009's executable conformance inventory."""

from __future__ import annotations

import json
from pathlib import Path
import re
from typing import Any

from activity_authority_boundary import validate_privileged_writers


ENUM_MANIFESTS = (
    (
        "commands.json", "rust/crates/pod0-application/src/contract.rs",
        "ApplicationCommand",
        {"variant", "domain", "target_owner", "request_disposition_fact",
         "transition_fact_policy", "child_issue", "implementation_status"},
    ),
    (
        "host-requests.json", "rust/crates/pod0-application/src/effects.rs",
        "HostRequest",
        {"variant", "domain", "target_executor", "authorization_fact",
         "lease_required", "child_issue", "implementation_status"},
    ),
    (
        "host-observations.json",
        "rust/crates/pod0-application/src/effects/observation.rs",
        "HostObservation",
        {"variant", "domain", "target_owner", "outcome_fact",
         "exact_effect_identity_required", "child_issue",
         "implementation_status"},
    ),
)
ROUTER_MANIFESTS = (
    (
        "rust/crates/pod0-application/src/contract.rs",
        "ApplicationCommand",
        "rust/crates/pod0-application/src/activity_routing_command.rs",
        "Command",
    ),
    (
        "rust/crates/pod0-application/src/effects.rs",
        "HostRequest",
        "rust/crates/pod0-application/src/activity_routing_effect.rs",
        "Request",
    ),
    (
        "rust/crates/pod0-application/src/effects/observation.rs",
        "HostObservation",
        "rust/crates/pod0-application/src/activity_routing_observation.rs",
        "Observation",
    ),
)
SQL_MUTATION = re.compile(
    r"\b(?:INSERT\s+(?:OR\s+\w+\s+)?INTO|UPDATE|DELETE\s+FROM|"
    r"REPLACE\s+INTO)\s+pod0_", re.IGNORECASE,
)
RECOVERY_CALL = re.compile(
    r"\b(?:rehydrate|recover|reconcile|retry_pending|resume_staged|admit_)"
    r"\w*\s*\("
)
NATIVE_EXECUTION = re.compile(
    r"protocol Core\w+Hosting|func start\w*Task\s*\(|func execute\s*\("
)
NATIVE_EXECUTION_EXCLUSIONS = {
    # Application-command/projection adapters whose generic `execute` methods
    # are not native HostRequest capability execution.
    "App/Sources/Core/SharedAgentConversationRuntime.swift",
    "App/Sources/Core/SharedLibraryClient+Commands.swift",
}
ALLOWED_ISSUES = set(range(205, 220))
MUTATION_EXCLUSIONS = {"migration.rs", "migration_db.rs", "migration_journal.rs"}
class DuplicateKeyError(ValueError):
    pass


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(key)
        result[key] = value
    return result


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=reject_duplicate_keys,
    )


def enum_body(source: str, enum_name: str) -> str:
    match = re.search(rf"pub enum {re.escape(enum_name)}\s*\{{", source)
    if match is None:
        raise ValueError(f"enum {enum_name} not found")
    start = match.end()
    depth = 1
    for index in range(start, len(source)):
        character = source[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return source[start:index]
    raise ValueError(f"enum {enum_name} has no closing brace")


def enum_variants(source: str, enum_name: str) -> list[str]:
    body = enum_body(source, enum_name)
    chunks: list[str] = []
    chunk_start = 0
    depth = 0
    for index, character in enumerate(body):
        if character in "{([":
            depth += 1
        elif character in "})]":
            depth -= 1
        elif character == "," and depth == 0:
            chunks.append(body[chunk_start:index])
            chunk_start = index + 1
    chunks.append(body[chunk_start:])
    variants: list[str] = []
    for chunk in chunks:
        match = re.search(
            r"(?:^|\n)\s*([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|$)",
            chunk.strip(),
        )
        if match is not None:
            variants.append(match.group(1))
    return variants


def detected_storage_mutators(root: Path) -> set[str]:
    result: set[str] = set()
    for path in (root / "rust/crates/pod0-storage/src").glob("*.rs"):
        name = path.name
        if (
            "test" in name or name.startswith("schema")
            or name in MUTATION_EXCLUSIONS or "fixture" in name
            or "backup" in name or "rollback" in name
        ):
            continue
        if SQL_MUTATION.search(path.read_text(encoding="utf-8")):
            result.add(path.relative_to(root).as_posix())
    return result


def detected_recovery_modules(root: Path) -> set[str]:
    result: set[str] = set()
    for path in (root / "rust/crates/pod0-facade/src").glob("runtime*.rs"):
        if "test" not in path.name and RECOVERY_CALL.search(
            path.read_text(encoding="utf-8")
        ):
            result.add(path.relative_to(root).as_posix())
    return result


def detected_native_executors(root: Path) -> set[str]:
    result: set[str] = set()
    for path in (root / "App/Sources/Core").glob("*.swift"):
        if NATIVE_EXECUTION.search(path.read_text(encoding="utf-8")):
            result.add(path.relative_to(root).as_posix())
    return result - NATIVE_EXECUTION_EXCLUSIONS


def exact_set_errors(
    label: str, actual: set[str], registered: set[str]
) -> list[str]:
    errors = [
        f"{label} missing inventory row: {item}"
        for item in sorted(actual - registered)
    ]
    errors += [
        f"{label} stale inventory row: {item}"
        for item in sorted(registered - actual)
    ]
    return errors


def validate_enums(root: Path, inventory_root: Path) -> list[str]:
    errors: list[str] = []
    for filename, source_path, enum_name, required in ENUM_MANIFESTS:
        try:
            data = load_json(inventory_root / filename)
            source = (root / source_path).read_text(encoding="utf-8")
            actual = enum_variants(source, enum_name)
        except (OSError, ValueError, json.JSONDecodeError, DuplicateKeyError) as error:
            errors.append(f"{filename}: {error}")
            continue
        items = data.get("items", [])
        registered = [item.get("variant") for item in items]
        if len(registered) != len(set(registered)):
            errors.append(f"{filename}: duplicate variant row")
        errors += exact_set_errors(enum_name, set(actual), set(registered))
        for item in items:
            missing = required - set(item)
            if missing:
                errors.append(
                    f"{filename}:{item.get('variant')}: missing {sorted(missing)}"
                )
            if item.get("child_issue") not in ALLOWED_ISSUES:
                errors.append(f"{filename}:{item.get('variant')}: invalid child issue")
            for field in required - {"lease_required", "child_issue"}:
                if item.get(field) in (None, ""):
                    errors.append(
                        f"{filename}:{item.get('variant')}: empty {field}"
                    )
    return errors


def validate_exhaustive_routers(root: Path) -> list[str]:
    errors: list[str] = []
    for enum_path, enum_name, router_path, prefix in ROUTER_MANIFESTS:
        try:
            variants = set(enum_variants(
                (root / enum_path).read_text(encoding="utf-8"), enum_name
            ))
            router = (root / router_path).read_text(encoding="utf-8")
        except (OSError, ValueError) as error:
            errors.append(f"{router_path}: {error}")
            continue
        routed = set(re.findall(rf"\b{prefix}::([A-Z][A-Za-z0-9_]*)", router))
        errors += exact_set_errors(f"{enum_name} router", variants, routed)
        if re.search(r"(?:^|[\s|])_\s*=>", router):
            errors.append(f"{router_path}: wildcard route is forbidden")
    return errors


def validate_surfaces(root: Path, inventory_root: Path) -> list[str]:
    try:
        surfaces = load_json(inventory_root / "surfaces.json").get("items", [])
    except (OSError, json.JSONDecodeError, DuplicateKeyError) as error:
        return [f"surfaces.json: {error}"]
    errors: list[str] = []
    keys = [(item.get("kind"), item.get("path")) for item in surfaces]
    if len(keys) != len(set(keys)):
        errors.append("surfaces.json: duplicate kind/path row")
    for item in surfaces:
        path = item.get("path")
        if not isinstance(path, str) or not (root / path).is_file():
            errors.append(f"surfaces.json: missing source path {path}")
        if item.get("child_issue") not in ALLOWED_ISSUES:
            errors.append(f"surfaces.json:{path}: invalid child issue")
        if not item.get("implementation_status"):
            errors.append(f"surfaces.json:{path}: empty implementation status")
    by_kind = {
        kind: {
            item["path"] for item in surfaces
            if item.get("kind") == kind and isinstance(item.get("path"), str)
        }
        for kind in (
            "authoritative_mutation_module", "recovery",
            "native_execution_module",
        )
    }
    errors += exact_set_errors(
        "authoritative mutation module", detected_storage_mutators(root),
        by_kind["authoritative_mutation_module"],
    )
    rust_recovery = {
        path for path in by_kind["recovery"] if path.startswith("rust/")
    }
    errors += exact_set_errors(
        "Rust recovery module", detected_recovery_modules(root), rust_recovery,
    )
    errors += exact_set_errors(
        "native execution module", detected_native_executors(root),
        by_kind["native_execution_module"],
    )
    return errors


def validate(root: Path) -> list[str]:
    inventory_root = root / "docs/architecture/activity-conformance"
    return (
        validate_enums(root, inventory_root)
        + validate_exhaustive_routers(root)
        + validate_surfaces(root, inventory_root)
        + validate_privileged_writers(root)
    )

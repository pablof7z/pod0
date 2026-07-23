#!/usr/bin/env python3
"""Keep current-state architecture documentation aligned with the Rust facade."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import tempfile


CONTRACT_PATTERN = re.compile(r"FACADE_CONTRACT_VERSION:\s*u32\s*=\s*(\d+)")
DOCUMENTED_PATTERN = re.compile(r"The facade contract is now version (\d+)")
HISTORICAL_DOCS = (
    "docs/spec/product-spec/03-design-architecture.md",
    "docs/spec/research/template-architecture-and-extension-plan.md",
)
STALE_CURRENT_CLAIMS = (
    "The NMP adapter remains isolated from the facade while the security hold",
    "Swift retains a temporary provider-schema formatter plus artifact and run-log storage",
    "currently owns subscription, download, transcript, knowledge, agent, Nostr, and artifact decisions",
    "Swift provider networking and polling clients",
)


class DuplicateKeyError(ValueError):
    """Raised when a machine-readable architecture document repeats a key."""


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(key)
        result[key] = value
    return result


def read(root: Path, relative_path: str) -> str:
    return (root / relative_path).read_text(encoding="utf-8")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    contract_source = read(root, "rust/crates/pod0-application/src/contract.rs")
    architecture = read(root, "docs/architecture.md")
    ownership_source = read(root, "docs/architecture/ownership.json")

    contract_match = CONTRACT_PATTERN.search(contract_source)
    documented_match = DOCUMENTED_PATTERN.search(architecture)
    if contract_match is None:
        errors.append("Rust facade contract version declaration was not found")
    if documented_match is None:
        errors.append("docs/architecture.md has no current facade contract version")
    if contract_match is not None and documented_match is not None:
        actual = int(contract_match.group(1))
        documented = int(documented_match.group(1))
        if actual != documented:
            errors.append(
                f"facade contract documentation drift: Rust={actual}, docs={documented}"
            )

    try:
        json.loads(ownership_source, object_pairs_hook=reject_duplicate_keys)
    except DuplicateKeyError as error:
        errors.append(f"ownership.json contains duplicate key: {error}")
    except json.JSONDecodeError as error:
        errors.append(f"ownership.json is invalid JSON: {error}")

    current_sources = architecture + "\n" + ownership_source
    for stale_claim in STALE_CURRENT_CLAIMS:
        if stale_claim in current_sources:
            errors.append(f"stale current-state architecture claim: {stale_claim}")

    for relative_path in HISTORICAL_DOCS:
        opening = "\n".join(read(root, relative_path).splitlines()[:12])
        if "Historical planning record" not in opening:
            errors.append(f"{relative_path} is not marked as a historical planning record")

    adr = read(
        root,
        "docs/architecture/adr/0008-agent-actions-permissions-and-nmp-publication.md",
    )
    if "The interactive agent currently receives" in adr:
        errors.append("ADR-0008 presents its pre-migration context as current behavior")
    if "## Implementation status" not in adr:
        errors.append("ADR-0008 has no implementation-status section")

    return errors


def write_fixture(root: Path, *, documented: int, duplicate_key: bool = False) -> None:
    paths = {
        "rust/crates/pod0-application/src/contract.rs":
            "pub const FACADE_CONTRACT_VERSION: u32 = 44;\n",
        "docs/architecture.md": f"The facade contract is now version {documented}.\n",
        "docs/architecture/ownership.json":
            '{"entries": [], "entries": []}\n' if duplicate_key else '{"entries": []}\n',
        HISTORICAL_DOCS[0]: "# Spec\n\n> Historical planning record; not current.\n",
        HISTORICAL_DOCS[1]: "# Research\n\n> Historical planning record; not current.\n",
        "docs/architecture/adr/0008-agent-actions-permissions-and-nmp-publication.md":
            "# ADR\n\n## Implementation status\n\nCurrent.\n",
    }
    for relative_path, content in paths.items():
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")


def run_self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, documented=44)
        if validate(root):
            print("architecture docs self-test valid fixture failed", file=sys.stderr)
            return 1

        write_fixture(root, documented=43)
        errors = validate(root)
        if not any("documentation drift" in error for error in errors):
            print("architecture docs self-test missed contract drift", file=sys.stderr)
            return 1

        write_fixture(root, documented=44, duplicate_key=True)
        errors = validate(root)
        if not any("duplicate key" in error for error in errors):
            print("architecture docs self-test missed duplicate JSON key", file=sys.stderr)
            return 1

    print("Architecture documentation negative fixtures passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
        help="repository root",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return run_self_test()

    errors = validate(Path(args.root).resolve())
    if errors:
        print("Architecture documentation check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Architecture documentation matches the current facade and ownership map")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

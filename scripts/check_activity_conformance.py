#!/usr/bin/env python3
"""Keep ADR-0009's ingress and bypass inventory exact and non-growing."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

from activity_conformance_lib import validate


def write_fixture(root: Path, extra_command: bool) -> None:
    sources = {
        "rust/crates/pod0-application/src/contract.rs":
            "pub enum ApplicationCommand { Known, NewVariant, }\n"
            if extra_command else "pub enum ApplicationCommand { Known, }\n",
        "rust/crates/pod0-application/src/effects.rs":
            "pub enum HostRequest { Known, }\n",
        "rust/crates/pod0-application/src/effects/observation.rs":
            "pub enum HostObservation { Known, }\n",
        "rust/crates/pod0-application/src/activity_routing_command.rs":
            "fn owner(value: Command) { match value { Command::Known => () } }\n",
        "rust/crates/pod0-application/src/activity_routing_effect.rs":
            "fn owner(value: Request) { match value { Request::Known => () } }\n",
        "rust/crates/pod0-application/src/activity_routing_observation.rs":
            "fn owner(value: Observation) { match value { Observation::Known => () } }\n",
    }
    for path, content in sources.items():
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
    inventory = root / "docs/architecture/activity-conformance"
    inventory.mkdir(parents=True)
    rows = {
        "commands.json": {
            "variant": "Known", "domain": "test", "target_owner": "TestMachine",
            "request_disposition_fact": "test.disposition",
            "transition_fact_policy": "required_when_state_changes",
            "child_issue": 205, "implementation_status": "planned",
        },
        "host-requests.json": {
            "variant": "Known", "domain": "test", "target_executor": "TestHost",
            "authorization_fact": "test.authorized", "lease_required": True,
            "child_issue": 205, "implementation_status": "planned",
        },
        "host-observations.json": {
            "variant": "Known", "domain": "test", "target_owner": "TestMachine",
            "outcome_fact": "test.observed",
            "exact_effect_identity_required": True,
            "child_issue": 205, "implementation_status": "planned",
        },
    }
    for filename, row in rows.items():
        (inventory / filename).write_text(
            json.dumps({"items": [row]}), encoding="utf-8"
        )
    (inventory / "surfaces.json").write_text(
        json.dumps({"items": []}), encoding="utf-8"
    )
    (root / "rust/crates/pod0-storage/src").mkdir(parents=True)
    (root / "rust/crates/pod0-facade/src").mkdir(parents=True)
    (root / "App/Sources/Core").mkdir(parents=True)


def run_self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_command=False)
        if validate(root):
            print("activity conformance valid fixture failed", file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_command=True)
        errors = validate(root)
        if not any("NewVariant" in error for error in errors):
            print("activity conformance missed new command variant", file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_command=False)
        bypass = root / "rust/crates/pod0-storage/src/bypass.rs"
        bypass.write_text(
            'fn bypass() { let _ = "INSERT INTO pod0_activity_facts"; }',
            encoding="utf-8",
        )
        errors = validate(root)
        if not any("privileged activity writer" in error for error in errors):
            print("activity conformance missed privileged writer bypass", file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture(root, extra_command=False)
        bypass = root / "App/Sources/Features/Bypass.swift"
        bypass.parent.mkdir(parents=True, exist_ok=True)
        bypass.write_text("let host = CoreTranscriptHost()", encoding="utf-8")
        errors = validate(root)
        if not any("direct native capability" in error for error in errors):
            print("activity conformance missed native capability bypass", file=sys.stderr)
            return 1
    print("Activity conformance negative fixtures passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root", default=str(Path(__file__).resolve().parents[1])
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return run_self_test()
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Activity conformance check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Activity conformance inventory matches source")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

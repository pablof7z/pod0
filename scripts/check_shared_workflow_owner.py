#!/usr/bin/env python3
"""Keep durable workflow execution single-owned by the Rust core."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


DELETED_PATHS = (
    "App/Sources/Workflows/WorkCoordinator.swift",
    "App/Sources/Workflows/WorkflowExecutors.swift",
    "App/Sources/Workflows/WorkflowProcessReconstructionHarness.swift",
    "scripts/test-workflow-process-reconstruction.sh",
)

FORBIDDEN = (
    (re.compile(r"\bWorkCoordinator\b"), "native durable-work coordinator"),
    (re.compile(r"\bMetadataIndexJobExecutor\b"), "native metadata-index executor"),
    (
        re.compile(r"\bWorkflowProcessReconstructionHarness\b"),
        "retired native process-reconstruction harness",
    ),
    (
        re.compile(r"\bPOD0_WORKFLOW_HARNESS_PHASE\b"),
        "retired native workflow harness environment",
    ),
    (
        re.compile(r"\.\s*attach\s*\(\s*jobStore\s*:"),
        "production Swift JobStore projection attachment",
    ),
    (
        re.compile(r"DesiredJob\s*\([^)]*kind\s*:\s*\.metadataIndex\b", re.S),
        "native metadata-index workflow admission",
    ),
)

RUNTIME_TOKENS = (
    "Native opportunity adapter for Rust-owned durable workflows",
    "ensurePublisherChapters",
    "ensureTranscriptWorkflows",
    "ensureModelChapters",
    "reconcileScheduledAgents",
    "case .swiftJobStore:",
    "return .notAllowed",
)

RECOVERY_TESTS = {
    "rust/crates/pod0-facade/src/runtime_chapter_workflow_race_tests.rs":
        "process_restart_after_http_success_reissues_until_durable_commit",
    "rust/crates/pod0-facade/src/runtime_download_admission_tests.rs":
        "waiting_request_is_admitted_by_environment_and_survives_restart",
    "rust/crates/pod0-facade/src/transcript_workflow_cutover_tests.rs":
        "legacy_workflow_cutover_survives_each_restart_and_recovers_owned_work",
    "rust/crates/pod0-storage/src/feed_discovery_store_tests.rs":
        "feed_discovery_commit_is_exact_replayable_and_durable_across_restart",
    "rust/crates/pod0-facade/src/runtime_scheduled_agent_workflow_tests.rs":
        "requested_restart_reissues_exactly_once_and_accepted_restart_is_ambiguous",
    "rust/crates/pod0-facade/src/runtime_agent_tests.rs":
        "native_action_is_fenced_and_restart_never_blindly_replays_it",
    "rust/crates/pod0-facade/src/runtime_publication_tests.rs":
        "generated_episode_publication_persists_receipt_and_missing_signer_across_restart",
}


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def findings(relative: str, source: str) -> list[str]:
    code = strip_comments(source)
    errors: list[str] = []
    for pattern, description in FORBIDDEN:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {description}")
    return errors


def require_tokens(
    root: Path,
    relative: str,
    tokens: tuple[str, ...] | list[str],
    description: str,
) -> list[str]:
    path = root / relative
    if not path.is_file():
        return [f"{relative}: required {description} is missing"]
    source = path.read_text(encoding="utf-8")
    return [
        f"{relative}: required {description} token {token!r} is missing"
        for token in tokens
        if token not in source
    ]


def validate(root: Path) -> list[str]:
    errors = [
        f"{relative}: retired native workflow authority path exists"
        for relative in DELETED_PATHS
        if (root / relative).exists()
    ]
    for path in (root / "App/Sources").rglob("*.swift"):
        relative = path.relative_to(root).as_posix()
        errors.extend(findings(relative, path.read_text(encoding="utf-8")))

    errors.extend(require_tokens(
        root,
        "App/Sources/Workflows/WorkflowRuntime.swift",
        list(RUNTIME_TOKENS),
        "native opportunity adapter",
    ))
    errors.extend(require_tokens(
        root,
        "App/Sources/Workflows/WorkJob.swift",
        ["Decode-only value from the retired generic native coordinator."],
        "legacy decode boundary",
    ))
    errors.extend(require_tokens(
        root,
        "AppTests/Sources/WorkflowJobActionTests.swift",
        ["testRuntimeRejectsDecodeOnlySwiftJobActions", ".notAllowed"],
        "legacy action rejection test",
    ))

    detail = root / "App/Sources/Features/EpisodeDetail/EpisodeDetailView.swift"
    if detail.is_file() and ".metadataIndex" in detail.read_text(encoding="utf-8"):
        errors.append(f"{detail.relative_to(root)}: retired metadata work is still presented")

    for relative, test_name in RECOVERY_TESTS.items():
        errors.extend(require_tokens(root, relative, [test_name], "Rust recovery test"))
    errors.extend(require_tokens(
        root,
        "scripts/test-shared-workflow-recovery.sh",
        list(RECOVERY_TESTS.values()),
        "shared recovery runner",
    ))
    errors.extend(require_tokens(
        root,
        ".github/workflows/test.yml",
        ["./scripts/test-shared-workflow-recovery.sh"],
        "hosted recovery gate",
    ))
    errors.extend(require_tokens(
        root,
        "docs/architecture/ownership.json",
        [
            "Rust owns all active durable workflow intent, lifecycle, and artifact receipts",
            "Residual Swift workflow and artifact tables are inactive development-migration inputs",
        ],
        "durable workflow ownership declaration",
    ))
    return errors


def self_test() -> None:
    assert not findings("App/Sources/Good.swift", "// WorkCoordinator")
    samples = (
        "let coordinator = WorkCoordinator(jobStore: store)",
        "let executor = MetadataIndexJobExecutor(store: store)",
        "WorkflowProcessReconstructionHarness.runIfRequested()",
        'environment["POD0_WORKFLOW_HARNESS_PHASE"]',
        "client.attach(jobStore: store)",
        "DesiredJob(idempotencyKey: key, kind: .metadataIndex, subjectID: id)",
    )
    for sample in samples:
        assert findings("App/Sources/Bad.swift", sample), sample


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Shared-workflow owner negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Shared-workflow owner policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Shared-workflow owner policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

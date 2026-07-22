#!/usr/bin/env python3
"""Prevent Swift scheduled-agent authority from returning after issue #130."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


DELETED_PATHS = (
    "App/Sources/Workflows/ScheduledAgentRunJobExecutor.swift",
)

FORBIDDEN = (
    (re.compile(r"\bScheduledAgentRunJobExecutor\b"), "retired Swift executor"),
    (re.compile(r"\bDesiredStatePlanner\s*\(\s*\)\s*\.plan\b"), "Swift scheduling policy"),
    (re.compile(r"\b(?:markTaskRun|advanceCompletedScheduledOccurrences)\s*\("), "Swift recurrence writer"),
    (
        re.compile(r"DesiredJob\s*\([^)]*kind\s*:\s*\.scheduledAgentRun\b", re.S),
        "Swift scheduled-run admission",
    ),
    (re.compile(r"\bScheduledRunPayload\s*\("), "Swift scheduled-run construction"),
    (
        re.compile(r"agentScheduledTasks\s*\.\s*(?:append|removeAll|remove|insert)\s*\("),
        "direct Swift scheduled-task durable mutation",
    ),
    (
        re.compile(r"\.scheduledAgentRun\s*:\s*(?:scheduled|ScheduledAgentRunJobExecutor)\b"),
        "Swift coordinator registration",
    ),
    (
        re.compile(r"records\s*=\s*\[\s*record\s*\(\s*\.scheduledOutput\b"),
        "Swift scheduled artifact commit",
    ),
)

REQUIRED = {
    "App/Sources/Core/SharedLibraryBootstrap.swift": (
        "LegacyScheduledAgentWorkflowCutover.run",
    ),
    "App/Sources/Workflows/JobStore+Database.swift": (
        "$0 != .scheduledAgentRun",
    ),
    "App/Sources/State/Persistence+SharedScheduledAgents.swift": (
        "activateSharedScheduledAgentAuthority",
        "DELETE FROM jobs WHERE kind='scheduledAgentRun'",
        "DELETE FROM artifacts WHERE kind='scheduledOutput'",
    ),
    "App/Sources/Core/SharedLibraryClient+ScheduledAgents.swift": (
        ".ensureScheduledTask",
        ".updateScheduledTask",
        ".removeScheduledTask",
        ".retryScheduledRun",
        ".cancelScheduledRun",
    ),
    "rust/crates/pod0-application/src/contract.rs": (
        "RetryScheduledRun",
    ),
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


def validate(root: Path) -> list[str]:
    errors = [
        f"{relative}: deleted scheduled-agent authority path exists"
        for relative in DELETED_PATHS
        if (root / relative).exists()
    ]
    for path in (root / "App/Sources").rglob("*.swift"):
        relative = path.relative_to(root).as_posix()
        errors.extend(findings(relative, path.read_text(encoding="utf-8")))
    for relative, tokens in REQUIRED.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: required scheduled-agent boundary is missing")
            continue
        source = path.read_text(encoding="utf-8")
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    return errors


def self_test() -> None:
    assert not findings("App/Sources/Good.swift", "// ScheduledAgentRunJobExecutor")
    samples = (
        "let executor = ScheduledAgentRunJobExecutor()",
        "DesiredStatePlanner().plan(input)",
        "store.markTaskRun(id: id)",
        "store.advanceCompletedScheduledOccurrences(from: jobs)",
        "DesiredJob(idempotencyKey: key, kind: .scheduledAgentRun, subjectID: id)",
        "ScheduledRunPayload(taskID: id)",
        "state.agentScheduledTasks.append(task)",
        ".scheduledAgentRun: scheduled",
        "records = [record(.scheduledOutput, job: job)]",
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
        print("Scheduled-agent single-writer negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Scheduled-agent single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Scheduled-agent single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

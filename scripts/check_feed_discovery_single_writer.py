#!/usr/bin/env python3
"""Prevent Swift feed-discovery policy from returning after issues #159/#160."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


DELETED_PATHS = (
    "App/Sources/State/AppStateStore+FeedDiscovery.swift",
    "App/Sources/Workflows/Reconciler.swift",
    "App/Sources/Workflows/WorkflowArtifactVerifier.swift",
)

FORBIDDEN = (
    (re.compile(r"\bFeedDiscoveryJobExecutor\b"), "retired Swift feed executor"),
    (
        re.compile(r"\bNewEpisodeNotificationJobExecutor\b"),
        "retired Swift notification executor",
    ),
    (
        re.compile(r"\b(?:recordSharedFeedDiscovery|feedDiscoveryJobs)\s*\("),
        "Swift feed-discovery planner",
    ),
    (re.compile(r"\bNotificationJobPayload\b"), "retired Swift notification payload"),
    (re.compile(r"\bFeedDiscoveryPayload\b"), "retired Swift feed payload"),
    (
        re.compile(
            r"DesiredJob\s*\([^)]*kind\s*:\s*\."
            r"(?:feedDiscovery|newEpisodeNotification)\b",
            re.S,
        ),
        "Swift feed-discovery durable admission",
    ),
    (
        re.compile(
            r"\.(?:feedDiscovery|newEpisodeNotification)\s*:\s*"
            r"(?:FeedDiscovery|NewEpisodeNotification)"
        ),
        "Swift runtime executor registration",
    ),
    (
        re.compile(
            r"\b(?:maxNewEpisodeNotificationsPerRefresh|notifyNewEpisodes)\b"
        ),
        "native notification cap or delivery policy",
    ),
    (
        re.compile(r"\bstate\.settings\.notifyOnNewEpisodes\b"),
        "native durable notification setting",
    ),
)

REQUIRED = {
    "App/Sources/Core/SharedLibraryBootstrap.swift": (
        "LegacyFeedDiscoveryWorkflowCutover.run",
    ),
    "App/Sources/Core/SharedLibraryBootstrap+Preparation.swift": (
        "notificationHost: CoreNotificationHost()",
    ),
    "App/Sources/Core/LegacyFeedDiscoveryWorkflowBackup.swift": (
        "enum LegacyFeedDiscoveryJobKind",
        "struct LegacyFeedDiscoveryWorkJob",
        "enum LegacyFeedDiscoveryArtifactKind",
        "struct LegacyFeedDiscoveryArtifactRecord",
    ),
    "App/Sources/Workflows/JobStore+LegacyFeedDiscoveryRetirement.swift": (
        "readLegacyFeedDiscoveryRows",
        "LegacyFeedDiscoveryJobKind.init",
    ),
    "App/Sources/Core/CoreNotificationHost.swift": (
        "protocol CoreNotificationHosting",
        "final class CoreNotificationHost",
        "newEpisodeNotificationDelivered",
    ),
    "rust/crates/pod0-application/src/feed_discovery.rs": (
        "MAX_NEW_EPISODE_NOTIFICATIONS_PER_OCCURRENCE",
        "FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS",
        "FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS",
        "FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS",
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


def has_retired_work_job_kind(source: str) -> bool:
    body = re.search(
        r"enum\s+WorkJobKind\b[^\{]*\{(?P<body>.*?)\n\}",
        strip_comments(source),
        re.S,
    )
    return bool(body and re.search(
        r"\bcase\s+(?:feedDiscovery|newEpisodeNotification)\b",
        body.group("body"),
    ))


def has_retired_artifact_kind(source: str) -> bool:
    body = re.search(
        r"enum\s+ArtifactKind\b[^\{]*\{(?P<body>.*?)\n\}",
        strip_comments(source),
        re.S,
    )
    return bool(body and re.search(
        r"\bcase\s+(?:feedDiscovery|notificationDelivery)\b",
        body.group("body"),
    ))


def validate(root: Path) -> list[str]:
    errors = [
        f"{relative}: deleted feed-discovery authority path exists"
        for relative in DELETED_PATHS
        if (root / relative).exists()
    ]
    for path in (root / "App/Sources").rglob("*.swift"):
        relative = path.relative_to(root).as_posix()
        errors.extend(findings(relative, path.read_text(encoding="utf-8")))
    work_job = root / "App/Sources/Workflows/WorkJob.swift"
    if work_job.is_file() and has_retired_work_job_kind(
        work_job.read_text(encoding="utf-8")
    ):
        errors.append(
            "App/Sources/Workflows/WorkJob.swift: retired feed-discovery kind is representable"
        )
    artifact = root / "App/Sources/Workflows/ArtifactRepository.swift"
    if artifact.is_file() and has_retired_artifact_kind(
        artifact.read_text(encoding="utf-8")
    ):
        errors.append(
            "App/Sources/Workflows/ArtifactRepository.swift: retired feed-discovery artifact is representable"
        )
    for relative, tokens in REQUIRED.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: required feed-discovery boundary is missing")
            continue
        source = path.read_text(encoding="utf-8")
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    return errors


def self_test() -> None:
    assert not findings("App/Sources/Good.swift", "// FeedDiscoveryJobExecutor")
    samples = (
        "let executor = FeedDiscoveryJobExecutor()",
        "let executor = NewEpisodeNotificationJobExecutor()",
        "store.recordSharedFeedDiscovery(podcastID: id)",
        "store.feedDiscoveryJobs(podcastID: id)",
        "let payload = NotificationJobPayload(discoveredAt: now)",
        "let payload: FeedDiscoveryPayload",
        "DesiredJob(idempotencyKey: key, kind: .feedDiscovery, subjectID: id)",
        "DesiredJob(idempotencyKey: key, kind: .newEpisodeNotification, subjectID: id)",
        ".feedDiscovery: FeedDiscoveryJobExecutor()",
        "NotificationService.maxNewEpisodeNotificationsPerRefresh",
        "NotificationService.notifyNewEpisodes(episodes, podcast: podcast)",
        "state.settings.notifyOnNewEpisodes",
    )
    for sample in samples:
        assert findings("App/Sources/Bad.swift", sample), sample
    assert has_retired_work_job_kind(
        "enum WorkJobKind: String {\n case metadataIndex\n case feedDiscovery\n}"
    )
    assert not has_retired_work_job_kind(
        "enum LegacyFeedDiscoveryJobKind: String {\n case feedDiscovery\n}"
    )
    assert has_retired_artifact_kind(
        "enum ArtifactKind: String {\n case notificationDelivery\n}"
    )
    assert not has_retired_artifact_kind(
        "enum LegacyFeedDiscoveryArtifactKind: String {\n case notificationDelivery\n}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Feed-discovery single-writer negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Feed-discovery single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Feed-discovery single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Exact source locations allowed to hold activity write authority."""

from pathlib import Path


PRIVILEGED_TOKENS = {
    "append_activity_facts(": {
        "rust/crates/pod0-storage/src/activity_store.rs",
        "rust/crates/pod0-storage/src/transition_commit_planned.rs",
    },
    "JournalAppendAuthority(": {
        "rust/crates/pod0-storage/src/transition_commit.rs",
        "rust/crates/pod0-storage/src/transition_commit_planned.rs",
    },
    "INSERT INTO pod0_activity_facts": {
        "rust/crates/pod0-storage/src/activity_store.rs",
    },
    "INSERT INTO pod0_effect_intents": {
        "rust/crates/pod0-storage/src/transition_commit_write.rs",
    },
    "INSERT INTO pod0_internal_command_intents": {
        "rust/crates/pod0-storage/src/transition_commit_write.rs",
    },
    "INSERT INTO pod0_transition_receipts": {
        "rust/crates/pod0-storage/src/transition_commit_write.rs",
    },
}
NATIVE_DRAIN_TOKENS = {
    "nextHostRequests(": {"App/Sources/Core/CoreHostRequestReader.swift"},
    "nextLeasedHostRequests(": {"App/Sources/Core/CoreHostRequestReader.swift"},
    "recordHostObservation(": {"App/Sources/Core/CoreDurableObservationRecorder.swift"},
    "recordLeasedHostObservation(": {
        "App/Sources/Core/CoreDurableObservationRecorder.swift"
    },
    "Pod0NativeHostDispatcher(": {"App/Sources/Core/SharedLibraryClient.swift"},
}
CAPABILITY_CONSTRUCTORS = (
    "CoreAgentHost(", "CoreChapterModelHost(", "CoreDownloadHost(",
    "CoreFeedHost(", "CoreNotificationHost(", "CorePlaybackHost(",
    "CorePublisherChapterHost(", "CoreRecallHost(", "CoreScheduledAgentHost(",
    "CoreTranscriptHost(",
)
FORBIDDEN_NATIVE_AUDIT_TOKENS = (
    "EpisodeAuditLogStore",
    "EpisodeAuditEvent(",
)


def validate_privileged_writers(root: Path) -> list[str]:
    errors: list[str] = []
    source_root = root / "rust/crates"
    if not source_root.exists():
        return errors
    for path in source_root.rglob("*.rs"):
        if "test" in path.name:
            continue
        relative = path.relative_to(root).as_posix()
        source = path.read_text(encoding="utf-8")
        for token, allowed_paths in PRIVILEGED_TOKENS.items():
            if token in source and relative not in allowed_paths:
                errors.append(
                    f"privileged activity writer token {token!r} in {relative}"
                )
    app_root = root / "App/Sources"
    if not app_root.exists():
        return errors
    for path in app_root.rglob("*.swift"):
        relative = path.relative_to(root).as_posix()
        source = path.read_text(encoding="utf-8")
        for token, allowed_paths in NATIVE_DRAIN_TOKENS.items():
            if token in source and relative not in allowed_paths:
                errors.append(f"privileged native drain token {token!r} in {relative}")
        if "/Core/" not in relative:
            for token in CAPABILITY_CONSTRUCTORS:
                if token in source:
                    errors.append(f"direct native capability {token!r} in {relative}")
        if not relative.startswith("AppTests/"):
            for token in FORBIDDEN_NATIVE_AUDIT_TOKENS:
                if token in source:
                    errors.append(f"native audit authority {token!r} in {relative}")
    return errors

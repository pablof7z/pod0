#!/usr/bin/env python3
"""Prevent Swift library/playback policy ownership from returning."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


BANNED_FILES = {
    "App/Sources/Features/Player/PlaybackSessionPolicy.swift",
    "App/Sources/Features/Player/PlaybackState+AudioSession.swift",
    "App/Sources/Features/Player/PlaybackState+LegacyPersistence.swift",
    "App/Sources/Services/SubscriptionService+LegacyHelpers.swift",
    "App/Sources/State/AppStateStore+PositionDebounce.swift",
}

BANNED_SYMBOLS = {
    "legacy library mode": r"\bSharedLibraryMode\b|\bisSharedLibraryAuthoritative\b",
    "legacy playback writer": r"\bsetEpisodePlaybackPosition\b|\bflushPendingPositions\b",
    "legacy playback policy": r"\bPlaybackSessionPolicy\b|\btickLegacyPersistence\b",
    "legacy playback callback": (
        r"\bonPersistPosition\b|\bonEpisodeFinished\b|"
        r"\bonSegmentFinished\b|\bonFlushPositions\b"
    ),
    "legacy library writer": r"\bupsertEpisodes\b|\baddSubscriptions\b",
}

DURABLE_FIELD_ASSIGNMENT = re.compile(
    r"\.(?:playbackPosition|played|isStarred)\s*="
)
COLLECTION_ASSIGNMENT = re.compile(
    r"(?:\$0|state)\.(?:podcasts|subscriptions)\s*(?:=|\.append|\.remove)"
)
PROJECTION_WRITERS = {
    "App/Sources/State/AppStateStore+Reset.swift",
    "App/Sources/State/AppStateStore+SharedLibrary.swift",
}


def strip_comments(source: str) -> str:
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


def findings(relative: str, source: str) -> list[str]:
    code = strip_comments(source)
    errors: list[str] = []
    for label, expression in BANNED_SYMBOLS.items():
        for match in re.finditer(expression, code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {label}")
    if relative.startswith("App/Sources/State/"):
        for match in DURABLE_FIELD_ASSIGNMENT.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: direct listening-field assignment")
        if relative not in PROJECTION_WRITERS:
            for match in COLLECTION_ASSIGNMENT.finditer(code):
                line = code.count("\n", 0, match.start()) + 1
                errors.append(f"{relative}:{line}: direct library-collection assignment")
    return errors


def validate(root: Path) -> list[str]:
    errors = [f"obsolete production file exists: {path}" for path in sorted(BANNED_FILES)
              if (root / path).exists()]
    for path in sorted((root / "App/Sources").rglob("*.swift")):
        relative = path.relative_to(root).as_posix()
        errors.extend(findings(relative, path.read_text(encoding="utf-8")))
    return errors


def self_test() -> None:
    source = """
    // setEpisodePlaybackPosition is only a comment.
    store.setEpisodePlaybackPosition(id, position: 4)
    state.subscriptions.append(value)
    state.episodes[index].played = true
    """
    assert len(findings("App/Sources/State/Bad.swift", source)) == 3
    assert not findings(
        "App/Sources/State/AppStateStore+SharedLibrary.swift",
        "mutateState { $0.subscriptions = projection }",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Listening single-writer negative fixture passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Listening single-writer boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Listening single-writer boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

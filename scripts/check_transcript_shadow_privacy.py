#!/usr/bin/env python3
"""Prevent transcript payload text from entering #96 shadow diagnostics."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


SHADOW_FILES = (
    "App/Sources/Core/SharedLibraryClient+Transcripts.swift",
    "App/Sources/Core/SharedTranscriptShadowComparator.swift",
)
FORBIDDEN = (
    ".text, privacy:",
    "transcript.text",
    "segment.text, privacy:",
    "word.text, privacy:",
    "sourceRevision, privacy:",
    "provider, privacy:",
    "language, privacy:",
    "String(describing: error)",
)


def evaluate(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in sources.items():
        for token in FORBIDDEN:
            if token in source:
                errors.append(f"{path}: forbidden transcript diagnostic token {token!r}")
    bridge = sources.get(SHADOW_FILES[0], "")
    for required in ("categories", "artifactId.stableString", "transcriptContentDigest.stableString"):
        if required not in bridge:
            errors.append(f"{SHADOW_FILES[0]}: missing privacy-safe diagnostic {required!r}")
    return errors


def self_test() -> None:
    safe = {
        SHADOW_FILES[0]: (
            "logger.notice(categories); artifactId.stableString; "
            "transcriptContentDigest.stableString"
        ),
        SHADOW_FILES[1]: "compare values in memory",
    }
    assert not evaluate(safe)
    unsafe = dict(safe)
    unsafe[SHADOW_FILES[0]] += "\nlogger.notice(segment.text, privacy: .public)"
    assert any("segment.text" in error for error in evaluate(unsafe))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Transcript shadow privacy negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    sources = {
        path: (root / path).read_text(encoding="utf-8")
        for path in SHADOW_FILES
    }
    errors = evaluate(sources)
    if errors:
        print("Transcript shadow privacy policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Transcript shadow privacy policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

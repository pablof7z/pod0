#!/usr/bin/env python3
"""Prevent Swift transcript authority from returning after the Rust cutover."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


DELETED_PATHS = (
    "App/Sources/Services/TranscriptStore.swift",
    "App/Sources/Core/SharedTranscriptShadowComparator.swift",
)

FORBIDDEN = (
    (re.compile(r"\bTranscriptStore\b"), "deleted Swift transcript store"),
    (
        re.compile(r"\bSharedTranscriptShadow(?:Comparator|Observer)\b"),
        "deleted transcript shadow path",
    ),
    (
        re.compile(r"\bTranscriptArtifactEvidence\b"),
        "deleted Swift transcript evidence authority",
    ),
    (
        re.compile(r"\b(?:applyTranscriptEvent|setEpisodeTranscriptState)\s*\("),
        "deleted Swift transcript-state writer",
    ),
    (
        re.compile(r"\bkind\s*:\s*\.transcript\b"),
        "legacy transcript ArtifactRecord writer",
    ),
)

REQUIRED_TOKENS = {
    "App/Sources/Core/SharedLibraryClient+Transcripts.swift": (
        ".commitTranscript(",
        "facade.dispatch(",
        "facade.snapshot(",
        "TranscriptSummaryProjection",
    ),
    "App/Sources/Core/SharedTranscriptReader.swift": (
        "facade.snapshot(",
        "TranscriptProjectionScope",
        "maximumPageSize",
    ),
    "App/Sources/Core/SharedLibraryBootstrap.swift": (
        "sharedTranscriptStoreIsAuthoritative(",
        "stageLegacyTranscriptImport(",
        "verifyStagedLegacyTranscriptImport(",
        "commitStagedLegacyTranscriptImport(",
        "SharedLibraryBootstrapFailureCode.classify(error)",
    ),
    "App/Sources/Workflows/ArtifactRepository.swift": (
        "kind NOT IN ('transcript','chapters','adSegments')",
    ),
}

LOGGED_PAYLOAD = re.compile(
    r"logger\.[^\n]*(?:transcript|segment|word)\s*\.\s*text",
    re.IGNORECASE,
)

SENSITIVE_PATH_PARTS = (
    "Transcript",
    "SharedLibraryBootstrap",
    "AIChapterCompiler",
)

UNSAFE_DIAGNOSTICS = (
    (re.compile(r"String\s*\(\s*describing\s*:\s*error\b"), "raw error description"),
    (re.compile(r"\berror\s*\.\s*localizedDescription\b"), "localized raw error"),
    (re.compile(r"\\\(\s*error\b"), "raw error interpolation"),
    (re.compile(r"\b(?:print|os_log)\s*\("), "unstructured diagnostic sink"),
)


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
    for pattern, description in FORBIDDEN:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {description}")
    for match in LOGGED_PAYLOAD.finditer(code):
        line = code.count("\n", 0, match.start()) + 1
        errors.append(f"{relative}:{line}: transcript payload text in diagnostics")
    if any(part in relative for part in SENSITIVE_PATH_PARTS):
        for pattern, description in UNSAFE_DIAGNOSTICS:
            for match in pattern.finditer(code):
                line = code.count("\n", 0, match.start()) + 1
                errors.append(f"{relative}:{line}: prohibited {description}")
        lines = code.splitlines()
        for index, line_source in enumerate(lines):
            if not re.search(r"\b(?:\w*logger\w*)\s*\.\s*\w+\s*\(", line_source, re.I):
                continue
            window = "\n".join(lines[index : index + 9])
            if re.search(
                r"\b(?:transcript|segment|word)\w*[^\n]{0,160}\.\s*(?:text|words?)\b",
                window,
                re.I,
            ):
                errors.append(
                    f"{relative}:{index + 1}: transcript payload field in diagnostics"
                )
            if "absoluteString" in window:
                errors.append(f"{relative}:{index + 1}: full URL in transcript diagnostics")
    return errors


def validate(root: Path) -> list[str]:
    errors = [
        f"{relative}: deleted transcript authority path exists"
        for relative in DELETED_PATHS
        if (root / relative).exists()
    ]
    sources = {
        path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
        for path in (root / "App/Sources").rglob("*.swift")
    }
    for relative, source in sources.items():
        errors.extend(findings(relative, source))
    repository = sources.get("App/Sources/Workflows/ArtifactRepository.swift", "")
    kind_body = re.search(r"enum\s+ArtifactKind\b[^\{]*\{(?P<body>.*?)\n\}", repository, re.S)
    if kind_body and re.search(r"\bcase\s+transcript\b", kind_body.group("body")):
        errors.append("App/Sources/Workflows/ArtifactRepository.swift: transcript kind is representable")
    for relative, tokens in REQUIRED_TOKENS.items():
        source = sources.get(relative)
        if source is None:
            errors.append(f"{relative}: required typed transcript boundary is missing")
            continue
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    return errors


def self_test() -> None:
    assert not findings("App/Sources/Good.swift", "// TranscriptStore was deleted")
    samples = (
        "let store = TranscriptStore.shared",
        "SharedTranscriptShadowObserver.observe()",
        "store.applyTranscriptEvent(event)",
        "ArtifactRecord(kind: .transcript, subjectID: id)",
        "logger.notice(segment.text)",
        'logger.error("failed: \\(error)")',
        'logger.error("failed: \\(error.localizedDescription)")',
        'logger.notice("payload: \\(\n transcript.segments[0].text\n)")',
    )
    for sample in samples:
        assert findings("App/Sources/TranscriptBad.swift", sample), sample


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Transcript single-writer negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Transcript single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Transcript single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Keep the iOS chapter capability raw, transient, typed, and non-authoritative."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


CAPABILITY_FILES = (
    "App/Sources/Core/ChapterObservationCapability.swift",
    "App/Sources/Core/ChapterObservationCapabilityAdapter.swift",
    "App/Sources/Core/ChapterObservationCapabilityAdapter+Mapping.swift",
    "App/Sources/Core/CorePublisherChapterHost.swift",
    "App/Sources/Core/Pod0NativeHostDispatcher+PublisherChapters.swift",
    "App/Sources/Core/ChapterModelTransport.swift",
    "App/Sources/Core/ChapterModelTransport+Retry.swift",
)
MODEL_TRANSPORT = "App/Sources/Core/ChapterModelTransport.swift"
MODEL_RETRY_TRANSPORT = "App/Sources/Core/ChapterModelTransport+Retry.swift"

FORBIDDEN = (
    (re.compile(r"\bEpisode\s*\.\s*(?:Chapter|AdSegment)\b"), "canonical Swift chapter/ad construction"),
    (re.compile(r"\b(?:AppStateStore|ArtifactRepository|DerivedArtifactStagingStore)\b"), "durable Swift owner"),
    (re.compile(r"\b(?:UserDefaults|FileManager|CoreData|SQLite|GRDB)\b"), "durable storage API"),
    (re.compile(r"\b(?:setEpisode|applyChapter|persistChapter|saveChapter)\w*\s*\("), "direct chapter mutation"),
    (re.compile(r"\b(?:AIChapterCompiler|PodcastChapter\w*|ChapterParser\w*)\b"), "semantic chapter implementation"),
    (re.compile(r"\b(?:parse|decode|classify)Chapter\w*\s*\("), "semantic chapter operation"),
    (
        re.compile(
            r"\b(?:retry(?:count|policy|delay|attempt|scheduled|budget)|fallback)\w*\s*[\(:=]",
            re.I,
        ),
        "native retry/fallback policy",
    ),
    (re.compile(r"\b(?:Logger|os_log|print)\s*[\.(]"), "capability diagnostics sink"),
    (re.compile(r"String\s*\(\s*data\s*:"), "provider payload stringification"),
)

REQUIRED_TOKENS = {
    "App/Sources/Core/ChapterObservationCapability.swift": (
        "ChapterCapabilityRequestEnvelope",
        "ChapterCapabilityEvidence",
        "chapterObservationLimits()",
        "qualifyModelChapterObservation",
        "qualifyAgentComposedChapterObservation",
    ),
    "App/Sources/Core/ChapterObservationCapabilityAdapter.swift": (
        "activeTasks",
        "completedRequestIDs",
        "qualifier.limits()",
        "func cancel(cancellationID:",
        "func shutdown()",
        "activeTasks.removeValue",
    ),
    "App/Sources/Core/ChapterObservationCapabilityAdapter+Mapping.swift": (
        "ModelChapterObservation(",
        "SHA256.hash(data:",
        "generatedAt: request.generatedAt",
    ),
    "App/Sources/Core/CorePublisherChapterHost.swift": (
        "session.bytes(for:",
        "maximumResponseBytes",
        ".publisherChaptersFetched(",
        "httpStatus: status",
    ),
    "App/Sources/Core/Pod0NativeHostDispatcher+PublisherChapters.swift": (
        "notBefore.date.timeIntervalSince(now())",
        "Task.sleep(nanoseconds:",
        "publisherChapterHost.fetch(",
        "activeTasks.removeValue",
    ),
    MODEL_TRANSPORT: (
        "session.bytes(for:",
        "maximumCompletionBytes",
        "OpenRouterEnvelope",
        "OllamaEnvelope",
        "keyDecodingStrategy = .convertFromSnakeCase",
    ),
    MODEL_RETRY_TRANSPORT: (
        "static func httpFailure(",
        "retryAfterMilliseconds:",
        "86_400_000",
    ),
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
    for pattern, description in FORBIDDEN:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {description}")

    if relative not in {MODEL_TRANSPORT, MODEL_RETRY_TRANSPORT}:
        for token in ("JSONDecoder", "JSONSerialization"):
            if token in code:
                errors.append(f"{relative}: semantic JSON parsing is prohibited")
    else:
        if "JSONSerialization.jsonObject" in code:
            errors.append(f"{relative}: provider response must use typed envelope decoding")
        allowed_envelopes = {"OpenRouterEnvelope", "OllamaEnvelope"}
        decoded_types = set(re.findall(r"\.decode\(\s*(\w+)\.self", code))
        for decoded_type in decoded_types - allowed_envelopes:
            errors.append(
                f"{relative}: prohibited provider response type {decoded_type!r}"
            )
        for token in ('"chapters"', '"ads"', "ChapterArtifactInput"):
            if token in code:
                errors.append(f"{relative}: provider transport contains semantic token {token!r}")

    if relative not in {MODEL_TRANSPORT, MODEL_RETRY_TRANSPORT} and (
        "Date()" in code or "Date.init" in code
    ):
        errors.append(f"{relative}: native capability invents durable observation time")
    return errors


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in CAPABILITY_FILES:
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: required chapter capability file is missing")
            continue
        source = path.read_text(encoding="utf-8")
        errors.extend(findings(relative, source))
        for token in REQUIRED_TOKENS[relative]:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    return errors


def self_test() -> None:
    safe = "// AppStateStore and JSONDecoder are not used here\nlet bytes = Data()"
    assert not findings("App/Sources/Core/CorePublisherChapterHost.swift", safe)
    samples = (
        "let chapter = Episode.Chapter()",
        "AppStateStore.shared.setEpisode(id)",
        "let defaults = UserDefaults.standard",
        "retryCount = 1",
        "print(payload)",
        "let decoder = JSONDecoder()",
        "let generatedAt = Date()",
    )
    for sample in samples:
        assert findings("App/Sources/Core/ChapterObservationCapabilityAdapter.swift", sample), sample
    assert findings(MODEL_TRANSPORT, 'let key = "chapters"')
    assert findings(
        MODEL_TRANSPORT,
        "let value = try decoder.decode(SemanticChapter.self, from: data)",
    )
    assert not findings(MODEL_TRANSPORT, "let decoder = JSONDecoder()")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Chapter capability boundary negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Chapter capability boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Chapter capability boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

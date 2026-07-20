#!/usr/bin/env python3
"""Prevent deleted Swift recall decision owners from being reintroduced."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


FORBIDDEN = (
    (
        re.compile(
            r"\b(?:struct|class|actor|enum|protocol)\s+"
            r"(?:RAGSearch|ChunkBuilder|TranscriptHit|ChunkMatch|VectorArtifactReceipt|"
            r"VectorStore|VectorIndex|RecallCapabilityService)\b"
        ),
        "deleted Swift recall authority type",
    ),
    (re.compile(r"\b(?:static\s+)?func\s+rrf\s*\("), "Swift rank-fusion policy"),
    (re.compile(r"\bfunc\s+(?:hybridTopK|topK)\s*\("), "Swift final-ranking API"),
    (
        re.compile(r"\b(?:scheduleRecallShadow|recallShadowTasks|RecallShadowParity)\b"),
        "deleted recall shadow path",
    ),
    (re.compile(r"\bqueryTranscripts\s*\("), "legacy Swift transcript-query API"),
)

DELETED_PATHS = (
    "App/Sources/Knowledge/RAGSearch.swift",
    "App/Sources/Knowledge/ChunkBuilder.swift",
    "App/Sources/Knowledge/Chunk.swift",
    "App/Sources/Core/SharedLibraryClient+RecallShadow.swift",
    "App/Sources/Agent/AgentTools+TranscriptEvidence.swift",
    "App/Sources/Knowledge/VectorIndex.swift",
    "App/Sources/Knowledge/VectorIndex+CoreRecall.swift",
    "App/Sources/Knowledge/VectorIndex+CoreRecallRetrieval.swift",
    "App/Sources/Services/RecallCapabilityService.swift",
)

REQUIRED_TOKENS = {
    "App/Sources/Core/SharedLibraryClient+Recall.swift": (
        ".recallQuery(",
        ".cancelOperation(",
        "facade.subscribe(",
    ),
    "App/Sources/Features/Recall/RecallAnswerService.swift": (
        "RecallResultProjection",
        "RecallEvidenceProjection",
    ),
    "App/Sources/Core/CoreRecallHost.swift": (
        ".embedRecallSpans(",
        "RecallSpanEmbeddingObservation",
    ),
    "App/Sources/Services/RecallProviderService.swift": (
        "ProviderEmbeddingsClient",
        "owns no durable index",
    ),
}


def evaluate(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in sources.items():
        for pattern, description in FORBIDDEN:
            if pattern.search(source):
                errors.append(f"{path}: {description}")

    answer = sources.get("App/Sources/Features/Recall/RecallAnswerService.swift", "")
    for token in (".sorted(", ".sort(", ".prefix("):
        if token in answer:
            errors.append(
                "App/Sources/Features/Recall/RecallAnswerService.swift: "
                f"native evidence reorder/truncation token {token!r}"
            )

    recall_client = sources.get("App/Sources/Core/SharedLibraryClient+Recall.swift", "")
    if "Task.sleep" in recall_client:
        errors.append(
            "App/Sources/Core/SharedLibraryClient+Recall.swift: polling/sleep is forbidden"
        )
    return errors


def self_test() -> None:
    safe = {
        "App/Sources/Core/SharedLibraryClient+Recall.swift": "facade.subscribe(",
        "App/Sources/Features/Recall/RecallAnswerService.swift": "RecallResultProjection",
    }
    assert not evaluate(safe)
    for pattern, _ in FORBIDDEN:
        token = {
            "rrf": "static func rrf(",
            "hybridTopK": "func hybridTopK(",
            "scheduleRecallShadow": "scheduleRecallShadow",
            "queryTranscripts": "queryTranscripts(",
        }
        sample = next((value for key, value in token.items() if key in pattern.pattern), None)
        sample = sample or "struct RAGSearch"
        errors = evaluate({"App/Sources/Bad.swift": sample})
        assert errors, pattern.pattern
    assert evaluate({
        "App/Sources/Features/Recall/RecallAnswerService.swift": ".sorted("
    })
    assert evaluate({
        "App/Sources/Core/SharedLibraryClient+Recall.swift": "Task.sleep"
    })


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Recall single-writer negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    sources = {
        path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
        for path in (root / "App/Sources").rglob("*.swift")
    }
    errors = evaluate(sources)
    for relative in DELETED_PATHS:
        if (root / relative).exists():
            errors.append(f"{relative}: deleted authority path exists")
    for relative, tokens in REQUIRED_TOKENS.items():
        source = sources.get(relative)
        if source is None:
            errors.append(f"{relative}: required typed boundary is missing")
            continue
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")

    if errors:
        print("Recall single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Recall single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

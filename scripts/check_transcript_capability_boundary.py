#!/usr/bin/env python3
"""Keep the native transcript host transient, typed, bounded, and policy-free."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


CAPABILITY_FILES = (
    "App/Sources/Core/CoreTranscriptTransport.swift",
    "App/Sources/Core/CoreTranscriptHost.swift",
    "App/Sources/Core/CoreTranscriptHost+Failures.swift",
    "App/Sources/Core/LiveCoreTranscriptTransport.swift",
    "App/Sources/Core/LiveCoreTranscriptTransport+Publisher.swift",
    "App/Sources/Core/Pod0NativeHostDispatcher+Transcripts.swift",
    "App/Sources/Transcript/AssemblyAITranscriptClient+Status.swift",
)

FORBIDDEN = (
    (re.compile(r"\b(?:AppStateStore|JobStore|ArtifactRepository)\b"), "durable native owner"),
    (re.compile(r"\b(?:UserDefaults|CoreData|SQLite|GRDB)\b"), "durable storage API"),
    (re.compile(r"\bTranscriptIngestService\b"), "legacy transcript policy owner"),
    (re.compile(r"\bTask\s*\.\s*sleep\b"), "sleep-driven polling"),
    (re.compile(r"\bwhile\b"), "native polling loop"),
    (re.compile(r"\bfallback\w*\s*[(:=]", re.I), "native fallback policy"),
    (re.compile(r"\b(?:recordExternal|scheduleRetry|retryCount)\w*\s*\("), "native workflow policy"),
)

REQUIRED = {
    "App/Sources/Core/CoreTranscriptHost.swift": (
        "validateTranscriptCapabilityRequest",
        "validateTranscriptCapabilityObservation",
        "TranscriptObservationMapper.map",
        ".transcriptCapabilityObserved",
    ),
    "App/Sources/Core/LiveCoreTranscriptTransport.swift": (
        "case .submitProvider",
        "case .recoverProvider",
        "assemblyAI.observe(",
        "providerRecoveryUnavailable",
    ),
    "App/Sources/Core/LiveCoreTranscriptTransport+Publisher.swift": (
        "session.bytes(for:",
        "maximumResponseBytes",
        "Task.checkCancellation()",
    ),
    "App/Sources/Core/Pod0NativeHostDispatcher+Transcripts.swift": (
        "transcriptHost.execute(envelope.request)",
        "activeTasks.removeValue",
        "remember: false",
    ),
    "App/Sources/Transcript/AssemblyAITranscriptClient+Status.swift": (
        "func observe(",
        "request.httpMethod = \"GET\"",
        "case \"queued\", \"processing\"",
    ),
}


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//.*", "", source)


def findings(relative: str, source: str) -> list[str]:
    code = strip_comments(source)
    errors: list[str] = []
    for pattern, description in FORBIDDEN:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {description}")
    return errors


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in CAPABILITY_FILES:
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: required transcript capability file is missing")
            continue
        source = path.read_text(encoding="utf-8")
        errors.extend(findings(relative, source))
        for token in REQUIRED.get(relative, ()):
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    return errors


def self_test() -> None:
    safe = "// Task.sleep is forbidden\nlet bytes = Data()"
    assert not findings(CAPABILITY_FILES[0], safe)
    for sample in (
        "let store = JobStore()",
        "try await Task.sleep(for: .seconds(1))",
        "while pending { await observe() }",
        "fallbackProvider = provider",
        "service.scheduleRetry()",
    ):
        assert findings(CAPABILITY_FILES[0], sample), sample


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Transcript capability boundary negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Transcript capability boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Transcript capability boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Prevent Swift chapter/ad authority from returning after the Rust cutover."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


DELETED_PATHS = (
    "App/Sources/Services/AIChapterCompiler.swift",
    "App/Sources/Podcast/ChaptersClient.swift",
    "App/Sources/Core/ChapterPublisherTransport.swift",
    "App/Sources/Workflows/DerivedArtifactStagingStore.swift",
    "App/Sources/State/AppStateStore+AdSegments.swift",
    "App/Sources/Features/Player/PlaybackState+AdSkip.swift",
    "App/Sources/Core/ChapterModelPromptBuilder.swift",
)

FORBIDDEN = (
    (re.compile(r"\bsetEpisode(?:Chapters|AdSegments)\s*\("), "deleted Swift writer"),
    (re.compile(r"\b(?:AIChapterCompiler|DerivedArtifactStagingStore|ChaptersClient)\b"), "deleted Swift authority"),
    (re.compile(r"\bapplyAutoSkipAdsIfNeeded\s*\("), "native ad-skip policy"),
    (
        re.compile(r"\b(?:PublisherChaptersJobExecutor|PublisherChapterRequestPayload)\b"),
        "retired Swift publisher workflow authority",
    ),
    (
        re.compile(r"DesiredJob\s*\([^)]*kind\s*:\s*\.publisherChapters\b", re.S),
        "Swift publisher workflow scheduling",
    ),
    (re.compile(r"\bChapterModelPromptBuilder\b"), "retired Swift chapter prompt policy"),
    (re.compile(r"\bchapterCompilerInputVersion\b"), "retired Swift chapter version policy"),
    (
        re.compile(r"You analyse podcast episode transcripts"),
        "chapter model prompt contract outside Rust",
    ),
    (re.compile(r"encodeIfPresent\s*\(\s*(?:chapters|adSegments)\b"), "chapter/ad Codable output"),
    (
        re.compile(r"(?:current|history|markIntegrity)\s*\(\s*kind\s*:\s*\.(?:chapters|adSegments)\b"),
        "legacy chapter/ad ArtifactRepository access",
    ),
    (
        re.compile(r"ArtifactRecord\s*\([^)]*kind\s*:\s*\.(?:chapters|adSegments)\b", re.S),
        "legacy chapter/ad ArtifactRecord writer",
    ),
)

REQUIRED_TOKENS = {
    "App/Sources/Core/SharedLibraryClient+Chapters.swift": (
        ".commitChapter(",
        "facade.dispatch(",
        "facade.snapshot(",
        "verifyChapterWorkflowReceipt",
    ),
    "App/Sources/Core/SharedChapterReader.swift": (
        "facade.snapshot(",
        "maximumPageSize",
        "selectedArtifactInput",
    ),
    "App/Sources/Core/SharedLibraryBootstrap+Chapters.swift": (
        "sharedChapterStoreIsAuthoritative(",
        "stageLegacyChapterImport(",
        "verifyStagedLegacyChapterImport(",
        "commitStagedLegacyChapterImport(",
    ),
    "App/Sources/Workflows/ArtifactRepository.swift": (
        "kind NOT IN ('transcript','chapters','adSegments')",
    ),
    "App/Sources/Workflows/ChapterWorkflowExecutors.swift": (
        "chapterModelPlan(",
        "PlannedChapterModelRequest",
        "request.sourceVersion == context.job.inputVersion",
        "submitChapterObservation(",
        "SharedChapterWorkflowReceipt(",
        "ChapterObservationCapabilityAdapter",
        "expectedSelectionRevision: request.expectedChapterSelectionRevision",
    ),
    "App/Sources/Workflows/DesiredStatePlanner.swift": (
        "planChapterModelDesiredState(",
        "transcriptContentDigest:",
        "selectedChapterSource:",
    ),
    "App/Sources/Core/SharedLibraryClient.swift": (
        "facade.planChapterModelRequest(",
        "episodeId: EpisodeId(uuid: episodeID)",
    ),
    "App/Sources/Core/ChapterModelTransport.swift": (
        "request.systemPrompt",
        "request.userPrompt",
        "planned.responseFormat",
        "planned.maximumCompletionBytes",
    ),
    "App/Sources/Workflows/WorkflowRuntime.swift": (
        "removeJobs(kind: .publisherChapters)",
        "projection.authority == .sharedRustPublisherChapters",
    ),
    "App/Sources/Services/WorkflowClient.swift": (
        "attachPublisherChapterCore(",
        ".filter { $0.kind != .publisherChapters && $0.kind != .chapterArtifacts }",
    ),
    "App/Sources/Core/SharedLibraryClient+PublisherChapterWorkflows.swift": (
        ".ensurePublisherChapters(",
        ".retryPublisherChapters(",
        ".cancelPublisherChapters(",
        ".chapterWorkflows(episodeId:",
    ),
    "App/Sources/Core/CorePublisherChapterHost.swift": (
        "session.bytes(for: request)",
        ".publisherChaptersFetched(",
    ),
    "App/Sources/Features/Player/PlaybackState+Chapters.swift": (
        ".nextChapter(",
        ".previousChapter(",
        "chapterContext",
    ),
    "App/Sources/State/AppStateStore.swift": (
        "sharedChapterStoreIsAuthoritative(",
        "loadLegacyChapterAdjuncts: !chapterAuthorityActive",
    ),
    "App/Sources/Podcast/Episode.swift": (
        "decoder.userInfo[.loadLegacyChapterAdjuncts]",
        "chapters = loadLegacyChapterAdjuncts",
        "adSegments = loadLegacyChapterAdjuncts",
    ),
}

SHARED_POLICY_TOKENS = {
    "rust/crates/pod0-application/src/chapter_model_policy.rs": (
        "pub fn plan_chapter_model_desired_state",
        "pub fn plan_chapter_model_request",
        "pub struct PlannedChapterModelRequest",
    ),
    "rust/crates/pod0-application/src/chapter_model_policy_prompt.rs": (
        "GENERATION_SYSTEM_PROMPT",
        "ENRICHMENT_SYSTEM_PROMPT",
        "MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS",
    ),
    "rust/crates/pod0-facade/src/runtime_chapter_model_plan.rs": (
        "selected_artifact(episode_id)",
        "selected_chapter_artifact(episode_id)",
        "expected_chapter_selection_revision",
    ),
}

ALLOWED_MATCH_FILES: set[str] = set()


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def findings(relative: str, source: str) -> list[str]:
    if relative in ALLOWED_MATCH_FILES:
        return []
    code = strip_comments(source)
    errors: list[str] = []
    for pattern, description in FORBIDDEN:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            errors.append(f"{relative}:{line}: prohibited {description}")
    return errors


def validate(root: Path) -> list[str]:
    errors = [
        f"{relative}: deleted chapter authority path exists"
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
    if kind_body and re.search(r"\bcase\s+(?:chapters|adSegments)\b", kind_body.group("body")):
        errors.append("App/Sources/Workflows/ArtifactRepository.swift: chapter/ad kind is representable")
    for relative, tokens in REQUIRED_TOKENS.items():
        source = sources.get(relative)
        if source is None:
            errors.append(f"{relative}: required typed chapter boundary is missing")
            continue
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required boundary token {token!r} is missing")
    for relative, tokens in SHARED_POLICY_TOKENS.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: required Rust chapter model policy is missing")
            continue
        source = path.read_text(encoding="utf-8")
        for token in tokens:
            if token not in source:
                errors.append(f"{relative}: required shared policy token {token!r} is missing")
    return errors


def self_test() -> None:
    assert not findings("App/Sources/Good.swift", "// setEpisodeChapters(id)")
    samples = (
        "store.setEpisodeChapters(id, chapters: values)",
        "let compiler = AIChapterCompiler.shared",
        "try c.encodeIfPresent(adSegments, forKey: .adSegments)",
        "repository.current(kind: .chapters, subjectID: id)",
        "ArtifactRecord(kind: .adSegments, subjectID: id)",
        "applyAutoSkipAdsIfNeeded(at: time)",
        "let executor = PublisherChaptersJobExecutor()",
        "PublisherChapterRequestPayload(sourceURL: url)",
        "DesiredJob(idempotencyKey: key, kind: .publisherChapters, subjectID: id)",
        "ChapterModelPromptBuilder.build(input)",
        "Self.chapterCompilerInputVersion(input)",
        'let prompt = "You analyse podcast episode transcripts"',
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
        print("Chapter single-writer negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Chapter single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Chapter single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

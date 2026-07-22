"""Required typed boundaries for the chapter single-writer policy."""

REQUIRED_TOKENS = {
    "App/Sources/Core/SharedLibraryClient+Chapters.swift": (
        ".commitChapter(", "facade.dispatch(", "facade.snapshot(",
    ),
    "App/Sources/Core/SharedChapterReader.swift": (
        "facade.snapshot(", "maximumPageSize", "selectedArtifactInput",
    ),
    "App/Sources/Core/SharedLibraryBootstrap+Chapters.swift": (
        "sharedChapterStoreIsAuthoritative(", "stageLegacyChapterImport(",
        "verifyStagedLegacyChapterImport(", "commitStagedLegacyChapterImport(",
    ),
    "App/Sources/Workflows/ArtifactRepository.swift": (
        "kind NOT IN ('transcript','chapters','adSegments')",
    ),
    "App/Sources/Core/LegacyModelChapterWorkflowCutover.swift": (
        "facade.modelChapterCutover()", "facade.stageLegacyModelChapterCutover(",
        "facade.discardStagedLegacyModelChapterCutover(",
        "LegacyModelChapterWorkflowBackupManifest.load(",
        "snapshot.backup.publish(to: backupRoot)", "removeLegacyChapterJobs(",
        "matching: jobs", "facade.commitLegacyModelChapterCutover(",
    ),
    "App/Sources/Core/LegacyModelChapterWorkflowBackup.swift": (
        "LegacyModelChapterWorkflowBackupClassification",
        "LegacyWorkflowBackupStorage.publish(",
        "jobs.allSatisfy({ $0.kind == .chapterArtifacts })",
    ),
    "App/Sources/Core/LegacyModelChapterWorkflowSnapshot.swift": (
        "LegacyModelChapterCutoverCandidate(",
        "LegacySharedChapterWorkflowReceiptV1.self", ".ambiguous",
    ),
    "App/Sources/Core/LegacyPublisherChapterWorkflowRetirement.swift": (
        "safeIdempotentRederivation", "corruptUnsupportedEvidence",
        "LegacyWorkflowBackupStorage.publish(",
        "commitLegacyChapterWorkflowRetirement(",
        "verifyLegacyChapterWorkflowRetirement(",
    ),
    "App/Sources/Core/LegacyWorkflowBackupStorage.swift": (
        "FileManager.default.linkItem(",
        "integrityDigest: ArtifactRepository.hash(", "synchronizePublishedFile(",
    ),
    "App/Sources/Workflows/JobStore+LegacyChapterRetirement.swift": (
        "LegacyChapterWorkflowRetirementMarker", "BEGIN IMMEDIATE TRANSACTION",
        "DELETE FROM jobs WHERE kind=?",
        "insertLegacyChapterWorkflowRetirementMarker(",
    ),
    "App/Sources/Core/SharedLibraryBootstrap.swift": (
        "stage = .chapterWorkflowRetirement",
        "LegacyPublisherChapterWorkflowRetirement.run(",
    ),
    "App/Sources/Core/SharedLibraryClient+Commands.swift": (
        "facade.planChapterModelRequest(",
        "episodeId: EpisodeId(uuid: episodeID)",
    ),
    "App/Sources/Core/ChapterModelTransport.swift": (
        "request.systemPrompt", "request.userPrompt", "request.responseFormat",
        "request.maximumCompletionBytes",
    ),
    "App/Sources/Workflows/WorkflowRuntime.swift": (
        "projection.authority == .sharedRustPublisherChapters",
        "projection.authority == .sharedRustModelChapters", "ensureModelChapters(",
    ),
    "App/Sources/Services/WorkflowClient.swift": (
        "attachPublisherChapterCore(", "corePublisherJobsByID",
        "coreModelChapterJobsByID",
    ),
    "App/Sources/Workflows/WorkflowJobProjection.swift": (
        "enum WorkflowProjectionKind", "var swiftJobKind: WorkJobKind?",
        "case publisherChapters", "case chapterArtifacts",
    ),
    "App/Sources/Workflows/JobStore+Projections.swift": (
        "compactMap(\\.swiftJobKind)",
    ),
    "App/Sources/Core/SharedLibraryClient+PublisherChapterWorkflows.swift": (
        ".ensurePublisherChapters(", ".retryPublisherChapters(",
        ".cancelPublisherChapters(", ".chapterWorkflows(episodeId:",
    ),
    "App/Sources/Core/SharedLibraryClient+ModelChapterWorkflows.swift": (
        ".ensureModelChapters(", ".retryModelChapters(",
        ".cancelModelChapters(", ".chapterWorkflows(episodeId:",
    ),
    "App/Sources/Core/CorePublisherChapterHost.swift": (
        "session.bytes(for: request)", ".publisherChaptersFetched(",
    ),
    "App/Sources/Features/Player/PlaybackState+Chapters.swift": (
        ".nextChapter(", ".previousChapter(", "chapterContext",
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
        "GENERATION_SYSTEM_PROMPT", "ENRICHMENT_SYSTEM_PROMPT",
        "MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS",
    ),
    "rust/crates/pod0-facade/src/runtime_chapter_model_plan.rs": (
        "selected_artifact(episode_id)", "selected_chapter_artifact(episode_id)",
        "expected_chapter_selection_revision",
    ),
}

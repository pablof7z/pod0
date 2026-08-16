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
    "App/Sources/Core/SharedLibraryClient+Commands.swift": (
        "facade.planChapterModelRequest(",
        "episodeId: EpisodeId(uuid: episodeID)",
    ),
    "App/Sources/Core/ChapterModelTransport.swift": (
        "request.systemPrompt", "request.userPrompt", "request.responseFormat",
        "request.maximumCompletionBytes",
    ),
    "App/Sources/Workflows/WorkflowRuntime.swift": (
        "projection.token(for: action)", "executeWorkflowAction(token)",
        ".reconcileWorkflowOpportunity(",
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
        "publisherChapterWorkflows", ".chapterWorkflows(episodeId:",
    ),
    "App/Sources/Core/SharedLibraryClient+ModelChapterWorkflows.swift": (
        "modelChapterWorkflows", ".chapterWorkflows(episodeId:",
    ),
    "App/Sources/Core/CorePublisherChapterHost.swift": (
        "session.bytes(for: request)", ".publisherChaptersFetched(",
    ),
    "App/Sources/Features/Player/PlaybackState+Chapters.swift": (
        ".nextChapter(", ".previousChapter(", "chapterContext",
    ),
    "App/Sources/State/AppStateStartupPreparation.swift": (
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
    "rust/crates/pod0-facade/src/workflow_action_facade.rs": (
        "WorkflowActionToken", "RetryPublisherChapters", "CancelPublisherChapters",
        "RetryModelChapters", "CancelModelChapters",
    ),
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

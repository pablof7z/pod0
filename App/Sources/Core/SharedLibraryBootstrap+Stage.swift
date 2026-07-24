enum SharedLibraryBootstrapStage: String {
    case storePreparation
    case listening
    case notes
    case clips
    case transcriptInspection
    case transcriptStaging
    case transcriptVerification
    case transcriptCommit
    case chapterInspection
    case chapterStaging
    case chapterVerification
    case chapterCommit
    case modelChapterWorkflowCutover
    case chapterWorkflowRetirement
    case facade
    case recallConfiguration
    case downloadWorkflowCutover
    case transcriptWorkflowCutover
    case scheduledAgentWorkflowCutover
    case agentHistoryCutover
    case agentRunLogRetirement
    case agentMemoryCutover
    case agentActivityRetirement
}

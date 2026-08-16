enum SharedLibraryBootstrapStage: String {
    case storePreparation
    case listening
    case listeningInspection
    case listeningStaging
    case listeningVerification
    case listeningCommit
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
    case facade
    case recallConfiguration
    case downloadWorkflowCutover
    case feedDiscoveryWorkflowCutover
    case transcriptWorkflowCutover
}

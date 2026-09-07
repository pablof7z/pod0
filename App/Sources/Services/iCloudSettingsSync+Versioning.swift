import Pod0Core

struct iCloudProductSettingsSnapshot {
    let schemaVersion: UInt32
    let writerVersion: SettingsWriterVersion
    let values: ProductSettingsValues
}

extension iCloudSettingsSync {
    /// Namespaced keys for the raw iCloud transport and its Rust-consumed
    /// version evidence.
    enum Key: String, CaseIterable {
        case agentInitialModel = "sync.settings.llmModel"
        case agentInitialModelName = "sync.settings.llmModelName"
        case agentThinkingModel = "sync.settings.agentThinkingModel"
        case agentThinkingModelName = "sync.settings.agentThinkingModelName"
        case memoryCompilationModel = "sync.settings.memoryCompilationModel"
        case memoryCompilationModelName = "sync.settings.memoryCompilationModelName"
        case wikiModel = "sync.settings.wikiModel"
        case wikiModelName = "sync.settings.wikiModelName"
        case categorizationModel = "sync.settings.categorizationModel"
        case categorizationModelName = "sync.settings.categorizationModelName"
        case chapterCompilationModel = "sync.settings.chapterCompilationModel"
        case chapterCompilationModelName = "sync.settings.chapterCompilationModelName"
        case imageGenerationModel = "sync.settings.imageGenerationModel"
        case imageGenerationModelName = "sync.settings.imageGenerationModelName"
        case ollamaChatURL = "sync.settings.ollamaChatURL"
        case youtubeExtractorURL = "sync.settings.youtubeExtractorURL"
        case embeddingsModel = "sync.settings.embeddingsModel"
        case embeddingsModelName = "sync.settings.embeddingsModelName"
        case rerankerEnabled = "sync.settings.rerankerEnabled"
        case sttProvider = "sync.settings.sttProvider"
        case openRouterWhisperModel = "sync.settings.openRouterWhisperModel"
        case assemblyAISTTModel = "sync.settings.assemblyAISTTModel"
        case elevenLabsSTTModel = "sync.settings.elevenLabsSTTModel"
        case elevenLabsTTSModel = "sync.settings.elevenLabsTTSModel"
        case elevenLabsVoiceID = "sync.settings.elevenLabsVoiceID"
        case elevenLabsVoiceName = "sync.settings.elevenLabsVoiceName"
        case defaultPlaybackRate = "sync.settings.defaultPlaybackRate"
        case skipForwardSeconds = "sync.settings.skipForwardSeconds"
        case skipBackwardSeconds = "sync.settings.skipBackwardSeconds"
        case autoMarkPlayedAtEnd = "sync.settings.autoMarkPlayedAtEnd"
        case autoPlayNext = "sync.settings.autoPlayNext"
        case autoDeleteDownloadsAfterPlayed = "sync.settings.autoDeleteDownloadsAfterPlayed"
        case autoSkipAds = "sync.settings.autoSkipAds"
        case headphoneDoubleTapAction = "sync.settings.headphoneDoubleTapAction"
        case headphoneTripleTapAction = "sync.settings.headphoneTripleTapAction"
        case autoIngestPublisherTranscripts = "sync.settings.autoIngestPublisherTranscripts"
        case autoFallbackToScribe = "sync.settings.autoFallbackToScribe"
        case agentDisplayName = "sync.settings.agentDisplayName"
        case agentAvatarURLString = "sync.settings.agentAvatarURLString"
        case schemaVersion = "sync.settings.version.schema"
        case writerCounter = "sync.settings.version.counter"
        case writerWord0 = "sync.settings.version.writer.0"
        case writerWord1 = "sync.settings.version.writer.1"
        case writerWord2 = "sync.settings.version.writer.2"
        case writerWord3 = "sync.settings.version.writer.3"

        static let portable = allCases.filter {
            ![schemaVersion, writerCounter, writerWord0, writerWord1, writerWord2, writerWord3]
                .contains($0)
        }
    }
}

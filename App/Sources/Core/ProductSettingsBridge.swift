import Foundation
import Pod0Core

enum ProductSettingsBridgeError: Error {
    case invalidPlaybackRate
    case invalidSkipInterval
}

enum ProductSettingsBridge {
    static func nativeMetadataOnly(from settings: Settings) -> Settings {
        let defaults = try? values(from: Settings())
        guard let defaults else { return settings }
        return applying(defaults, to: settings)
    }

    static func values(from settings: Settings) throws -> ProductSettingsValues {
        let playbackRate = (settings.defaultPlaybackRate * 1_000).rounded()
        let avatar = settings.agentAvatarURLString.trimmed
        guard playbackRate.isFinite,
              playbackRate >= Double(UInt16.min),
              playbackRate <= Double(UInt16.max),
              let rate = UInt16(exactly: Int(playbackRate))
        else {
            throw ProductSettingsBridgeError.invalidPlaybackRate
        }
        guard let forward = UInt16(exactly: settings.skipForwardSeconds),
              let backward = UInt16(exactly: settings.skipBackwardSeconds)
        else {
            throw ProductSettingsBridgeError.invalidSkipInterval
        }
        return ProductSettingsValues(
            agentInitialModel: settings.agentInitialModel,
            agentInitialModelName: settings.agentInitialModelName,
            agentThinkingModel: settings.agentThinkingModel,
            agentThinkingModelName: settings.agentThinkingModelName,
            memoryCompilationModel: settings.memoryCompilationModel,
            memoryCompilationModelName: settings.memoryCompilationModelName,
            utilityModel: settings.wikiModel,
            utilityModelName: settings.wikiModelName,
            categorizationModel: settings.categorizationModel,
            categorizationModelName: settings.categorizationModelName,
            chapterCompilationModel: settings.chapterCompilationModel,
            chapterCompilationModelName: settings.chapterCompilationModelName,
            imageGenerationModel: settings.imageGenerationModel,
            imageGenerationModelName: settings.imageGenerationModelName,
            ollamaChatUrl: settings.ollamaChatURL,
            youtubeExtractorUrl: settings.youtubeExtractorURL,
            transcriptionProvider: transcriptionProvider(settings.sttProvider),
            openRouterWhisperModel: settings.openRouterWhisperModel,
            assemblyAiSttModel: settings.assemblyAISTTModel,
            elevenLabsSttModel: settings.elevenLabsSTTModel,
            elevenLabsTtsModel: settings.elevenLabsTTSModel,
            elevenLabsVoiceId: settings.elevenLabsVoiceID,
            elevenLabsVoiceName: settings.elevenLabsVoiceName,
            defaultPlaybackRateMilli: rate,
            skipForwardSeconds: forward,
            skipBackwardSeconds: backward,
            autoMarkPlayedAtEnd: settings.autoMarkPlayedAtEnd,
            autoDeleteDownloadsAfterPlayed: settings.autoDeleteDownloadsAfterPlayed,
            autoPlayNext: settings.autoPlayNext,
            autoSkipAds: settings.autoSkipAds,
            headphoneDoubleTapAction: headphoneAction(settings.headphoneDoubleTapAction),
            headphoneTripleTapAction: headphoneAction(settings.headphoneTripleTapAction),
            autoIngestPublisherTranscripts: settings.autoIngestPublisherTranscripts,
            autoFallbackToScribe: settings.autoFallbackToScribe,
            agentDisplayName: settings.agentDisplayName,
            agentAvatarUrl: avatar.isEmpty ? nil : avatar
        )
    }

    static func applying(_ values: ProductSettingsValues, to settings: Settings) -> Settings {
        var result = settings
        result.agentInitialModel = values.agentInitialModel
        result.agentInitialModelName = values.agentInitialModelName
        result.agentThinkingModel = values.agentThinkingModel
        result.agentThinkingModelName = values.agentThinkingModelName
        result.memoryCompilationModel = values.memoryCompilationModel
        result.memoryCompilationModelName = values.memoryCompilationModelName
        result.wikiModel = values.utilityModel
        result.wikiModelName = values.utilityModelName
        result.categorizationModel = values.categorizationModel
        result.categorizationModelName = values.categorizationModelName
        result.chapterCompilationModel = values.chapterCompilationModel
        result.chapterCompilationModelName = values.chapterCompilationModelName
        result.imageGenerationModel = values.imageGenerationModel
        result.imageGenerationModelName = values.imageGenerationModelName
        result.ollamaChatURL = values.ollamaChatUrl
        result.youtubeExtractorURL = values.youtubeExtractorUrl
        result.sttProvider = transcriptionProvider(values.transcriptionProvider)
        result.openRouterWhisperModel = values.openRouterWhisperModel
        result.assemblyAISTTModel = values.assemblyAiSttModel
        result.elevenLabsSTTModel = values.elevenLabsSttModel
        result.elevenLabsTTSModel = values.elevenLabsTtsModel
        result.elevenLabsVoiceID = values.elevenLabsVoiceId
        result.elevenLabsVoiceName = values.elevenLabsVoiceName
        result.defaultPlaybackRate = Double(values.defaultPlaybackRateMilli) / 1_000
        result.skipForwardSeconds = Int(values.skipForwardSeconds)
        result.skipBackwardSeconds = Int(values.skipBackwardSeconds)
        result.autoMarkPlayedAtEnd = values.autoMarkPlayedAtEnd
        result.autoDeleteDownloadsAfterPlayed = values.autoDeleteDownloadsAfterPlayed
        result.autoPlayNext = values.autoPlayNext
        result.autoSkipAds = values.autoSkipAds
        result.headphoneDoubleTapAction = headphoneAction(values.headphoneDoubleTapAction)
        result.headphoneTripleTapAction = headphoneAction(values.headphoneTripleTapAction)
        result.autoIngestPublisherTranscripts = values.autoIngestPublisherTranscripts
        result.autoFallbackToScribe = values.autoFallbackToScribe
        result.agentDisplayName = values.agentDisplayName
        result.agentAvatarURLString = values.agentAvatarUrl ?? ""
        return result
    }
}

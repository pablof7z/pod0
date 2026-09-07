import Pod0Core

extension ProductSettingsBridge {
    static func intents(
        from before: ProductSettingsValues,
        to settings: Settings
    ) throws -> [ProductSettingIntent] {
        let after = try values(from: settings)
        var intents: [ProductSettingIntent] = []
        appendModel(&intents, .agentInitial, before.agentInitialModel,
                    before.agentInitialModelName, after.agentInitialModel,
                    after.agentInitialModelName)
        appendModel(&intents, .agentThinking, before.agentThinkingModel,
                    before.agentThinkingModelName, after.agentThinkingModel,
                    after.agentThinkingModelName)
        appendModel(&intents, .memoryCompilation, before.memoryCompilationModel,
                    before.memoryCompilationModelName, after.memoryCompilationModel,
                    after.memoryCompilationModelName)
        appendModel(&intents, .utility, before.utilityModel, before.utilityModelName,
                    after.utilityModel, after.utilityModelName)
        appendModel(&intents, .categorization, before.categorizationModel,
                    before.categorizationModelName, after.categorizationModel,
                    after.categorizationModelName)
        appendModel(&intents, .chapterCompilation, before.chapterCompilationModel,
                    before.chapterCompilationModelName, after.chapterCompilationModel,
                    after.chapterCompilationModelName)
        appendModel(&intents, .imageGeneration, before.imageGenerationModel,
                    before.imageGenerationModelName, after.imageGenerationModel,
                    after.imageGenerationModelName)
        if before.ollamaChatUrl != after.ollamaChatUrl {
            intents.append(.setOllamaChatUrl(url: after.ollamaChatUrl))
        }
        if before.youtubeExtractorUrl != after.youtubeExtractorUrl {
            intents.append(.setYoutubeExtractorUrl(url: after.youtubeExtractorUrl))
        }
        if before.transcriptionProvider != after.transcriptionProvider {
            intents.append(.setTranscriptionProvider(provider: after.transcriptionProvider))
        }
        appendTranscriptionModel(&intents, .openRouterWhisper,
                                 before.openRouterWhisperModel, after.openRouterWhisperModel)
        appendTranscriptionModel(&intents, .assemblyAi,
                                 before.assemblyAiSttModel, after.assemblyAiSttModel)
        appendTranscriptionModel(&intents, .elevenLabs,
                                 before.elevenLabsSttModel, after.elevenLabsSttModel)
        if before.elevenLabsTtsModel != after.elevenLabsTtsModel {
            intents.append(.setTextToSpeechModel(modelId: after.elevenLabsTtsModel))
        }
        if before.elevenLabsVoiceId != after.elevenLabsVoiceId
            || before.elevenLabsVoiceName != after.elevenLabsVoiceName {
            intents.append(.setTextToSpeechVoice(
                voiceId: after.elevenLabsVoiceId,
                voiceName: after.elevenLabsVoiceName
            ))
        }
        appendPlaybackIntents(&intents, before, after)
        appendTranscriptIntents(&intents, before, after)
        if before.agentDisplayName != after.agentDisplayName
            || before.agentAvatarUrl != after.agentAvatarUrl {
            intents.append(.setAgentIdentity(
                displayName: after.agentDisplayName,
                avatarUrl: after.agentAvatarUrl
            ))
        }
        return intents
    }

    private static func appendModel(
        _ intents: inout [ProductSettingIntent], _ slot: ProductModelSlot,
        _ oldID: String, _ oldName: String, _ newID: String, _ newName: String
    ) {
        guard oldID != newID || oldName != newName else { return }
        intents.append(.selectModel(slot: slot, modelId: newID, modelName: newName))
    }

    private static func appendTranscriptionModel(
        _ intents: inout [ProductSettingIntent], _ slot: TranscriptionModelSlot,
        _ old: String, _ new: String
    ) {
        guard old != new else { return }
        intents.append(.setTranscriptionModel(slot: slot, modelId: new))
    }

    private static func appendPlaybackIntents(
        _ intents: inout [ProductSettingIntent],
        _ before: ProductSettingsValues, _ after: ProductSettingsValues
    ) {
        if before.defaultPlaybackRateMilli != after.defaultPlaybackRateMilli {
            intents.append(.setPlaybackRate(milli: after.defaultPlaybackRateMilli))
        }
        if before.skipForwardSeconds != after.skipForwardSeconds
            || before.skipBackwardSeconds != after.skipBackwardSeconds {
            intents.append(.setSkipIntervals(
                forwardSeconds: after.skipForwardSeconds,
                backwardSeconds: after.skipBackwardSeconds
            ))
        }
        appendToggle(&intents, .autoMarkPlayedAtEnd,
                     before.autoMarkPlayedAtEnd, after.autoMarkPlayedAtEnd)
        appendToggle(&intents, .autoDeleteDownloadsAfterPlayed,
                     before.autoDeleteDownloadsAfterPlayed,
                     after.autoDeleteDownloadsAfterPlayed)
        appendToggle(&intents, .autoPlayNext, before.autoPlayNext, after.autoPlayNext)
        appendToggle(&intents, .autoSkipAds, before.autoSkipAds, after.autoSkipAds)
        if before.headphoneDoubleTapAction != after.headphoneDoubleTapAction {
            intents.append(.setHeadphoneGesture(tap: .double,
                                                 action: after.headphoneDoubleTapAction))
        }
        if before.headphoneTripleTapAction != after.headphoneTripleTapAction {
            intents.append(.setHeadphoneGesture(tap: .triple,
                                                 action: after.headphoneTripleTapAction))
        }
    }

    private static func appendToggle(
        _ intents: inout [ProductSettingIntent], _ setting: PlaybackSettingToggle,
        _ old: Bool, _ new: Bool
    ) {
        guard old != new else { return }
        intents.append(.setPlaybackToggle(setting: setting, enabled: new))
    }

    private static func appendTranscriptIntents(
        _ intents: inout [ProductSettingIntent],
        _ before: ProductSettingsValues, _ after: ProductSettingsValues
    ) {
        if before.autoIngestPublisherTranscripts != after.autoIngestPublisherTranscripts {
            intents.append(.setTranscriptToggle(
                setting: .autoIngestPublisherTranscripts,
                enabled: after.autoIngestPublisherTranscripts
            ))
        }
        if before.autoFallbackToScribe != after.autoFallbackToScribe {
            intents.append(.setTranscriptToggle(
                setting: .autoFallbackToScribe,
                enabled: after.autoFallbackToScribe
            ))
        }
    }
}

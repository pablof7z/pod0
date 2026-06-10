// PodcastSettingsSnapshot.generated.swift
// Hand-maintained mirror of `ffi::projections::SettingsSnapshot`.
// Source of truth: apps/nmp-app-podcast/src/ffi/projections/settings.rs

import Foundation

/// App-settings projection. Mirrors `ffi::projections::SettingsSnapshot`.
struct SettingsSnapshot: Equatable {
    var hasCompletedOnboarding: Bool = false
    var autoSkipAdsEnabled: Bool = true
    var autoPlayNext: Bool = true
    var autoMarkPlayedAtEnd: Bool = true
    var headphoneDoubleTapAction: String = "skipForward"
    var headphoneTripleTapAction: String = "clipNow"
    var skipForwardSecs: Double = 30
    var skipBackwardSecs: Double = 15
    var defaultPlaybackRate: Double = 1.0
    var autoDeleteDownloadsAfterPlayed: Bool = false
    var agentInitialModel: String = "deepseek-v4-flash:cloud"
    var agentInitialModelName: String = "DeepSeek Flash"
    var agentThinkingModel: String = "deepseek-v4-pro:cloud"
    var agentThinkingModelName: String = "DeepSeek Pro"
    var memoryCompilationModel: String = "deepseek-v4-flash:cloud"
    var memoryCompilationModelName: String = "DeepSeek Flash"
    var wikiModel: String = "deepseek-v4-flash:cloud"
    var wikiModelName: String = "DeepSeek Flash"
    var categorizationModel: String = "deepseek-v4-flash:cloud"
    var categorizationModelName: String = "DeepSeek Flash"
    var chapterCompilationModel: String = "deepseek-v4-flash:cloud"
    var chapterCompilationModelName: String = "DeepSeek Flash"
    var embeddingsModel: String = "deepseek-v4-flash:cloud"
    var embeddingsModelName: String = "DeepSeek Flash"
    var imageGenerationModel: String = "google/gemini-2.5-flash-image"
    var imageGenerationModelName: String = "Gemini 2.5 Flash"
    var rerankerEnabled: Bool = false
    var openRouterCredentialSource: String = ""
    var openRouterKeyPresent: Bool = false
    var openRouterBYOKKeyID: String? = nil
    var openRouterBYOKKeyLabel: String? = nil
    var openRouterConnectedAt: Date? = nil
    var ollamaCredentialSource: String = ""
    var ollamaKeyPresent: Bool = false
    var ollamaBYOKKeyID: String? = nil
    var ollamaBYOKKeyLabel: String? = nil
    var ollamaConnectedAt: Date? = nil
    var ollamaChatURL: String = "https://ollama.com/api/chat"
    var elevenLabsCredentialSource: String = ""
    var elevenLabsKeyPresent: Bool = false
    var elevenLabsBYOKKeyID: String? = nil
    var elevenLabsBYOKKeyLabel: String? = nil
    var elevenLabsConnectedAt: Date? = nil
    var assemblyAICredentialSource: String = ""
    var assemblyAIKeyPresent: Bool = false
    var assemblyAIBYOKKeyID: String? = nil
    var assemblyAIBYOKKeyLabel: String? = nil
    var assemblyAIConnectedAt: Date? = nil
    var perplexityCredentialSource: String = ""
    var perplexityKeyPresent: Bool = false
    var perplexityBYOKKeyID: String? = nil
    var perplexityBYOKKeyLabel: String? = nil
    var perplexityConnectedAt: Date? = nil
    var sttProvider: String = "apple_native"
    var effectiveSttProvider: String = "apple_native"
    var effectiveSttProviderRequiresKey: Bool = false
    var openRouterWhisperModel: String = "openai/whisper-1"
    var assemblyAISTTModel: String = "universal-3-pro,universal-2"
    var elevenLabsSTTModel: String = "scribe_v1"
    var elevenLabsTTSModel: String = "eleven_turbo_v2_5"
    var elevenLabsVoiceID: String = ""
    var elevenLabsVoiceName: String = ""
    var blossomServerURL: String = "https://blossom.primal.net"
    var youtubeExtractorURL: String? = nil
    var localModelID: String? = nil
    var wikiAutoGenerateOnTranscriptIngest: Bool = false
    var autoIngestPublisherTranscripts: Bool = true
    var autoFallbackToScribe: Bool = true
    var notifyOnNewEpisodes: Bool = true
    var nostrEnabled: Bool = false
    var nostrRelayURL: String = ""
    var nostrPublicRelays: [String] = []
    var nostrProfileName: String = ""
    var nostrProfileAbout: String = ""
    var nostrProfilePicture: String = ""
    var nostrPublicKeyHex: String? = nil
}

extension SettingsSnapshot: Codable {
    enum CodingKeys: String, CodingKey {
        case hasCompletedOnboarding
        case autoSkipAdsEnabled
        case autoPlayNext
        case autoMarkPlayedAtEnd
        case headphoneDoubleTapAction
        case headphoneTripleTapAction
        case skipForwardSecs
        case skipBackwardSecs
        case defaultPlaybackRate
        case autoDeleteDownloadsAfterPlayed
        case agentInitialModel
        case agentInitialModelName
        case agentThinkingModel
        case agentThinkingModelName
        case memoryCompilationModel
        case memoryCompilationModelName
        case wikiModel
        case wikiModelName
        case categorizationModel
        case categorizationModelName
        case chapterCompilationModel
        case chapterCompilationModelName
        case embeddingsModel
        case embeddingsModelName
        case imageGenerationModel
        case imageGenerationModelName
        case rerankerEnabled
        case openRouterCredentialSource
        case openRouterKeyPresent
        case openRouterBYOKKeyID = "openRouterByokKeyId"
        case openRouterBYOKKeyLabel = "openRouterByokKeyLabel"
        case openRouterConnectedAt
        case ollamaCredentialSource
        case ollamaKeyPresent
        case ollamaBYOKKeyID = "ollamaByokKeyId"
        case ollamaBYOKKeyLabel = "ollamaByokKeyLabel"
        case ollamaConnectedAt
        case ollamaChatURL = "ollama_chat_url"
        case elevenLabsCredentialSource
        case elevenLabsKeyPresent
        case elevenLabsBYOKKeyID = "elevenLabsByokKeyId"
        case elevenLabsBYOKKeyLabel = "elevenLabsByokKeyLabel"
        case elevenLabsConnectedAt
        case assemblyAICredentialSource = "assemblyAiCredentialSource"
        case assemblyAIKeyPresent = "assemblyAiKeyPresent"
        case assemblyAIBYOKKeyID = "assemblyAiByokKeyId"
        case assemblyAIBYOKKeyLabel = "assemblyAiByokKeyLabel"
        case assemblyAIConnectedAt = "assemblyAiConnectedAt"
        case perplexityCredentialSource
        case perplexityKeyPresent
        case perplexityBYOKKeyID = "perplexityByokKeyId"
        case perplexityBYOKKeyLabel = "perplexityByokKeyLabel"
        case perplexityConnectedAt
        case sttProvider = "stt_provider"
        case effectiveSttProvider
        case effectiveSttProviderRequiresKey
        case openRouterWhisperModel = "open_router_whisper_model"
        case assemblyAISTTModel = "assembly_ai_stt_model"
        case elevenLabsSTTModel = "eleven_labs_stt_model"
        case elevenLabsTTSModel = "eleven_labs_tts_model"
        case elevenLabsVoiceID = "eleven_labs_voice_id"
        case elevenLabsVoiceName = "eleven_labs_voice_name"
        case blossomServerURL = "blossom_server_url"
        case youtubeExtractorURL = "youtube_extractor_url"
        case localModelID = "local_model_id"
        case wikiAutoGenerateOnTranscriptIngest = "wiki_auto_generate_on_transcript_ingest"
        case autoIngestPublisherTranscripts = "auto_ingest_publisher_transcripts"
        case autoFallbackToScribe = "auto_fallback_to_scribe"
        case notifyOnNewEpisodes = "notify_on_new_episodes"
        case nostrEnabled = "nostr_enabled"
        case nostrRelayURL = "nostr_relay_url"
        case nostrPublicRelays = "nostr_public_relays"
        case nostrProfileName = "nostr_profile_name"
        case nostrProfileAbout = "nostr_profile_about"
        case nostrProfilePicture = "nostr_profile_picture"
        case nostrPublicKeyHex = "nostr_public_key_hex"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        hasCompletedOnboarding = try c.decodeIfPresent(Bool.self, forKey: .hasCompletedOnboarding) ?? false
        autoSkipAdsEnabled = try c.decodeIfPresent(Bool.self, forKey: .autoSkipAdsEnabled) ?? true
        autoPlayNext = try c.decodeIfPresent(Bool.self, forKey: .autoPlayNext) ?? true
        autoMarkPlayedAtEnd = try c.decodeIfPresent(Bool.self, forKey: .autoMarkPlayedAtEnd) ?? true
        headphoneDoubleTapAction = try c.decodeIfPresent(String.self, forKey: .headphoneDoubleTapAction) ?? "skipForward"
        headphoneTripleTapAction = try c.decodeIfPresent(String.self, forKey: .headphoneTripleTapAction) ?? "clipNow"
        skipForwardSecs = try c.decodeIfPresent(Double.self, forKey: .skipForwardSecs) ?? 30
        skipBackwardSecs = try c.decodeIfPresent(Double.self, forKey: .skipBackwardSecs) ?? 15
        defaultPlaybackRate = try c.decodeIfPresent(Double.self, forKey: .defaultPlaybackRate) ?? 1.0
        autoDeleteDownloadsAfterPlayed = try c.decodeIfPresent(Bool.self, forKey: .autoDeleteDownloadsAfterPlayed) ?? false
        agentInitialModel = try c.decodeIfPresent(String.self, forKey: .agentInitialModel) ?? "deepseek-v4-flash:cloud"
        agentInitialModelName = try c.decodeIfPresent(String.self, forKey: .agentInitialModelName) ?? "DeepSeek Flash"
        agentThinkingModel = try c.decodeIfPresent(String.self, forKey: .agentThinkingModel) ?? "deepseek-v4-pro:cloud"
        agentThinkingModelName = try c.decodeIfPresent(String.self, forKey: .agentThinkingModelName) ?? "DeepSeek Pro"
        memoryCompilationModel = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModel) ?? "deepseek-v4-flash:cloud"
        memoryCompilationModelName = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModelName) ?? "DeepSeek Flash"
        wikiModel = try c.decodeIfPresent(String.self, forKey: .wikiModel) ?? "deepseek-v4-flash:cloud"
        wikiModelName = try c.decodeIfPresent(String.self, forKey: .wikiModelName) ?? "DeepSeek Flash"
        categorizationModel = try c.decodeIfPresent(String.self, forKey: .categorizationModel) ?? "deepseek-v4-flash:cloud"
        categorizationModelName = try c.decodeIfPresent(String.self, forKey: .categorizationModelName) ?? "DeepSeek Flash"
        chapterCompilationModel = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModel) ?? "deepseek-v4-flash:cloud"
        chapterCompilationModelName = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModelName) ?? "DeepSeek Flash"
        embeddingsModel = try c.decodeIfPresent(String.self, forKey: .embeddingsModel) ?? "deepseek-v4-flash:cloud"
        embeddingsModelName = try c.decodeIfPresent(String.self, forKey: .embeddingsModelName) ?? "DeepSeek Flash"
        imageGenerationModel = try c.decodeIfPresent(String.self, forKey: .imageGenerationModel) ?? "google/gemini-2.5-flash-image"
        imageGenerationModelName = try c.decodeIfPresent(String.self, forKey: .imageGenerationModelName) ?? "Gemini 2.5 Flash"
        rerankerEnabled = try c.decodeIfPresent(Bool.self, forKey: .rerankerEnabled) ?? false
        try decodeCredentialMetadata(c, "openRouter")
        ollamaChatURL = try c.decodeIfPresent(String.self, forKey: .ollamaChatURL) ?? "https://ollama.com/api/chat"
        try decodeCredentialMetadata(c, "ollama")
        try decodeCredentialMetadata(c, "elevenLabs")
        try decodeCredentialMetadata(c, "assemblyAI")
        try decodeCredentialMetadata(c, "perplexity")
        sttProvider = try c.decodeIfPresent(String.self, forKey: .sttProvider) ?? "apple_native"
        effectiveSttProvider = try c.decodeIfPresent(String.self, forKey: .effectiveSttProvider) ?? "apple_native"
        effectiveSttProviderRequiresKey = try c.decodeIfPresent(Bool.self, forKey: .effectiveSttProviderRequiresKey) ?? false
        openRouterWhisperModel = try c.decodeIfPresent(String.self, forKey: .openRouterWhisperModel) ?? "openai/whisper-1"
        assemblyAISTTModel = try c.decodeIfPresent(String.self, forKey: .assemblyAISTTModel) ?? "universal-3-pro,universal-2"
        elevenLabsSTTModel = try c.decodeIfPresent(String.self, forKey: .elevenLabsSTTModel) ?? "scribe_v1"
        elevenLabsTTSModel = try c.decodeIfPresent(String.self, forKey: .elevenLabsTTSModel) ?? "eleven_turbo_v2_5"
        elevenLabsVoiceID = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceID) ?? ""
        elevenLabsVoiceName = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceName) ?? ""
        blossomServerURL = try c.decodeIfPresent(String.self, forKey: .blossomServerURL) ?? "https://blossom.primal.net"
        youtubeExtractorURL = try c.decodeIfPresent(String.self, forKey: .youtubeExtractorURL)
        localModelID = try c.decodeIfPresent(String.self, forKey: .localModelID)
        wikiAutoGenerateOnTranscriptIngest = try c.decodeIfPresent(Bool.self, forKey: .wikiAutoGenerateOnTranscriptIngest) ?? false
        autoIngestPublisherTranscripts = try c.decodeIfPresent(Bool.self, forKey: .autoIngestPublisherTranscripts) ?? true
        autoFallbackToScribe = try c.decodeIfPresent(Bool.self, forKey: .autoFallbackToScribe) ?? true
        notifyOnNewEpisodes = try c.decodeIfPresent(Bool.self, forKey: .notifyOnNewEpisodes) ?? true
        nostrEnabled = try c.decodeIfPresent(Bool.self, forKey: .nostrEnabled) ?? false
        nostrRelayURL = try c.decodeIfPresent(String.self, forKey: .nostrRelayURL) ?? ""
        nostrPublicRelays = try c.decodeIfPresent([String].self, forKey: .nostrPublicRelays) ?? []
        nostrProfileName = try c.decodeIfPresent(String.self, forKey: .nostrProfileName) ?? ""
        nostrProfileAbout = try c.decodeIfPresent(String.self, forKey: .nostrProfileAbout) ?? ""
        nostrProfilePicture = try c.decodeIfPresent(String.self, forKey: .nostrProfilePicture) ?? ""
        nostrPublicKeyHex = try c.decodeIfPresent(String.self, forKey: .nostrPublicKeyHex)
    }

    private mutating func decodeCredentialMetadata(
        _ c: KeyedDecodingContainer<CodingKeys>,
        _ provider: String
    ) throws {
        switch provider {
        case "openRouter":
            openRouterCredentialSource = try c.decodeIfPresent(String.self, forKey: .openRouterCredentialSource) ?? ""
            openRouterKeyPresent = try c.decodeIfPresent(Bool.self, forKey: .openRouterKeyPresent) ?? false
            openRouterBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyID)
            openRouterBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyLabel)
            openRouterConnectedAt = try decodeDate(c, .openRouterConnectedAt)
        case "ollama":
            ollamaCredentialSource = try c.decodeIfPresent(String.self, forKey: .ollamaCredentialSource) ?? ""
            ollamaKeyPresent = try c.decodeIfPresent(Bool.self, forKey: .ollamaKeyPresent) ?? false
            ollamaBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyID)
            ollamaBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyLabel)
            ollamaConnectedAt = try decodeDate(c, .ollamaConnectedAt)
        case "elevenLabs":
            elevenLabsCredentialSource = try c.decodeIfPresent(String.self, forKey: .elevenLabsCredentialSource) ?? ""
            elevenLabsKeyPresent = try c.decodeIfPresent(Bool.self, forKey: .elevenLabsKeyPresent) ?? false
            elevenLabsBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyID)
            elevenLabsBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyLabel)
            elevenLabsConnectedAt = try decodeDate(c, .elevenLabsConnectedAt)
        case "assemblyAI":
            assemblyAICredentialSource = try c.decodeIfPresent(String.self, forKey: .assemblyAICredentialSource) ?? ""
            assemblyAIKeyPresent = try c.decodeIfPresent(Bool.self, forKey: .assemblyAIKeyPresent) ?? false
            assemblyAIBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyID)
            assemblyAIBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyLabel)
            assemblyAIConnectedAt = try decodeDate(c, .assemblyAIConnectedAt)
        case "perplexity":
            perplexityCredentialSource = try c.decodeIfPresent(String.self, forKey: .perplexityCredentialSource) ?? ""
            perplexityKeyPresent = try c.decodeIfPresent(Bool.self, forKey: .perplexityKeyPresent) ?? false
            perplexityBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyID)
            perplexityBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyLabel)
            perplexityConnectedAt = try decodeDate(c, .perplexityConnectedAt)
        default:
            break
        }
    }

    private func decodeDate(
        _ c: KeyedDecodingContainer<CodingKeys>,
        _ key: CodingKeys
    ) throws -> Date? {
        guard let timestamp = try c.decodeIfPresent(Int.self, forKey: key) else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(timestamp))
    }
}

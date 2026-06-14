import Foundation

// MARK: - Settings

enum OpenRouterCredentialSource: String, Codable, Hashable, Sendable {
    case none, manual, byok
}

enum ElevenLabsCredentialSource: String, Codable, Hashable, Sendable {
    case none, manual, byok
}

enum OllamaCredentialSource: String, Codable, Hashable, Sendable {
    case none, manual, byok
}

enum AssemblyAICredentialSource: String, Codable, Hashable, Sendable {
    case none, manual, byok
}

enum PerplexityCredentialSource: String, Codable, Hashable, Sendable {
    case none, manual, byok
}

/// Action mapped to a headphone-remote multi-tap gesture (AirPods double/triple
/// tap or stem squeeze, wired earbud inline button). iOS converts these into
/// standard `MPRemoteCommandCenter.nextTrackCommand` / `.previousTrackCommand`
/// events — this enum decides what each one does inside the podcast player.
///
/// Enabling the underlying remote commands also makes a next/previous-track
/// button visible on the lock screen, Control Center, and CarPlay — whatever
/// the gesture is mapped to fires from those surfaces too.
enum HeadphoneGestureAction: String, Codable, Hashable, Sendable, CaseIterable {
    case skipForward
    case skipBackward
    case nextChapter
    case previousChapter
    case clipNow
    case none

    var displayName: String {
        switch self {
        case .skipForward:     return "Skip Forward"
        case .skipBackward:    return "Skip Back"
        case .nextChapter:     return "Next Chapter"
        case .previousChapter: return "Previous Chapter"
        case .clipNow:         return "Clip Current Position"
        case .none:            return "Do Nothing"
        }
    }
}

enum STTProvider: String, Codable, Hashable, Sendable, CaseIterable {
    case elevenLabsScribe = "elevenlabs_scribe"
    case assemblyAI = "assemblyai"
    case openRouterWhisper = "openrouter_whisper"
    /// Apple's on-device `SpeechTranscriber` (iOS 26+). No API key required;
    /// uses Apple Silicon neural engine. Requires a locally downloaded episode.
    case appleNative = "apple_native"

    var displayName: String {
        switch self {
        case .elevenLabsScribe: return "ElevenLabs Scribe"
        case .assemblyAI: return "AssemblyAI"
        case .openRouterWhisper: return "OpenRouter Whisper"
        case .appleNative: return "Apple on-device"
        }
    }
}

struct Settings: Codable, Hashable, Sendable {

    // MARK: - Kernel-canonical defaults
    /// Single Swift-side mirror of the kernel's fresh-install defaults. Every
    /// pre-first-frame `Settings` default that has a snapshot counterpart reads
    /// from here, so the only literal defaults that remain live in
    /// `SettingsSnapshot`'s property initializers (the generated mirror of
    /// `PodcastStore::new()`). A cross-language fixture test pins the two.
    private static let kernelDefaults = SettingsSnapshot()

    /// Default Ollama chat endpoint (Ollama Cloud). Users can override this to
    /// point at a local or self-hosted instance from Settings → Providers → Ollama.
    static var defaultOllamaChatURL: String { kernelDefaults.ollamaChatURL }

    // AI / LLM
    /// Model the agent chat session starts on. Designed to be a cheap/fast model
    /// — the agent decides per-task whether to call `upgrade_thinking`, which
    /// switches the session over to `agentThinkingModel` for subsequent turns.
    var agentInitialModel: String = Settings.kernelDefaults.agentInitialModel
    var agentInitialModelName: String = Settings.kernelDefaults.agentInitialModelName
    /// Stronger model the agent escalates to via the `upgrade_thinking` tool when
    /// a task needs more reasoning than the initial model can reliably provide.
    var agentThinkingModel: String = Settings.kernelDefaults.agentThinkingModel
    var agentThinkingModelName: String = Settings.kernelDefaults.agentThinkingModelName
    var memoryCompilationModel: String = Settings.kernelDefaults.memoryCompilationModel
    var memoryCompilationModelName: String = Settings.kernelDefaults.memoryCompilationModelName
    /// Model used by `WikiGenerator`. Kept distinct so users can pick a
    /// cheaper / faster model for wiki compilation than for live agent chat — same pattern
    /// as `memoryCompilationModel`.
    var wikiModel: String = Settings.kernelDefaults.wikiModel
    var wikiModelName: String = Settings.kernelDefaults.wikiModelName
    /// Model used by `PodcastCategorizationService`. Kept distinct so users can pick a
    /// cheaper model for one-shot categorization without affecting live agent chat.
    var categorizationModel: String = Settings.kernelDefaults.categorizationModel
    var categorizationModelName: String = Settings.kernelDefaults.categorizationModelName
    /// Model used by the kernel's `podcast.chapters.compile` action to synthesise
    /// chapter boundaries from a ready transcript. Kept distinct from `wikiModel`
    /// so users can pick a cheaper / faster model for compile without affecting wiki quality.
    var chapterCompilationModel: String = Settings.kernelDefaults.chapterCompilationModel
    var chapterCompilationModelName: String = Settings.kernelDefaults.chapterCompilationModelName
    var embeddingsModel: String = Settings.kernelDefaults.embeddingsModel
    var embeddingsModelName: String = Settings.kernelDefaults.embeddingsModelName
    /// Model used by `ImageGenerationService`. Multimodal models (Gemini/Banana,
    /// GPT-image) route through /chat/completions; legacy DALL-E/FLUX use
    /// /images/generations. Defaults to Gemini 2.5 Flash Image ("Nano Banana").
    var imageGenerationModel: String = Settings.kernelDefaults.imageGenerationModel
    var imageGenerationModelName: String = Settings.kernelDefaults.imageGenerationModelName
    /// When `true`, optionally re-rank top-k RAG candidates with a cross-encoder. Off by
    /// default to save tokens; settings UI exposes the toggle.
    var rerankerEnabled: Bool = Settings.kernelDefaults.rerankerEnabled
    /// ID of the selected local model (e.g. "gemma-4-e2b"), or nil to use cloud providers.
    /// When set, all AI features route through the on-device LiteRT-LM model.
    var localModelID: String?

    // Blossom
    /// Blossom BUD-02 server used for uploading podcast artwork, audio, chapters, and
    /// transcripts. Defaults to blossom.primal.net. Configurable in Settings > Agent.
    var blossomServerURL: String = Settings.kernelDefaults.blossomServerURL

    // OpenRouter credentials (secret stored in Keychain; only metadata here)
    var openRouterCredentialSource: OpenRouterCredentialSource =
        OpenRouterCredentialSource(rawValue: Settings.kernelDefaults.openRouterCredentialSource) ?? .none
    var openRouterBYOKKeyID: String?
    var openRouterBYOKKeyLabel: String?
    var openRouterConnectedAt: Date?

    // Ollama Cloud credentials (secret stored in Keychain; only metadata here)
    var ollamaCredentialSource: OllamaCredentialSource =
        OllamaCredentialSource(rawValue: Settings.kernelDefaults.ollamaCredentialSource) ?? .none
    var ollamaBYOKKeyID: String?
    var ollamaBYOKKeyLabel: String?
    var ollamaConnectedAt: Date?
    /// Chat endpoint for Ollama requests. Defaults to the Ollama Cloud API so
    /// existing users see no change. Set to e.g. `http://localhost:11434/api/chat`
    /// to point at a local instance. Stored as a String so partial edits during
    /// typing don't break `Codable`; validated as a URL at the network call site.
    var ollamaChatURL: String = Settings.defaultOllamaChatURL

    // YouTube ingestion
    /// Endpoint of a self-hosted YouTube audio-extraction service (e.g. cobalt,
    /// yt-dlp wrapper). The agent's `youtube_ingestion` skill POSTs
    /// `{"url": "<youtube_url>"}` here and expects back JSON with at least an
    /// `audio_url` (or `url`) field. `nil` means the skill is unavailable.
    var youtubeExtractorURL: String?

    // ElevenLabs credentials (secret stored in Keychain; only metadata here)
    var elevenLabsCredentialSource: ElevenLabsCredentialSource =
        ElevenLabsCredentialSource(rawValue: Settings.kernelDefaults.elevenLabsCredentialSource) ?? .none
    var elevenLabsBYOKKeyID: String?
    var elevenLabsBYOKKeyLabel: String?
    var elevenLabsConnectedAt: Date?

    // AssemblyAI credentials (secret stored in Keychain; only metadata here)
    var assemblyAICredentialSource: AssemblyAICredentialSource =
        AssemblyAICredentialSource(rawValue: Settings.kernelDefaults.assemblyAICredentialSource) ?? .none
    var assemblyAIBYOKKeyID: String?
    var assemblyAIBYOKKeyLabel: String?
    var assemblyAIConnectedAt: Date?

    // Perplexity credentials (secret stored in Keychain; only metadata here)
    var perplexityCredentialSource: PerplexityCredentialSource =
        PerplexityCredentialSource(rawValue: Settings.kernelDefaults.perplexityCredentialSource) ?? .none
    var perplexityBYOKKeyID: String?
    var perplexityBYOKKeyLabel: String?
    var perplexityConnectedAt: Date?

    // STT provider selection
    var sttProvider: STTProvider =
        STTProvider(rawValue: Settings.kernelDefaults.sttProvider) ?? .appleNative
    /// Whisper model used when `sttProvider == .openRouterWhisper`. Must be a model
    /// accessible on OpenRouter's audio transcription endpoint.
    var openRouterWhisperModel: String = Settings.kernelDefaults.openRouterWhisperModel
    /// Comma-separated AssemblyAI speech models submitted to `/v2/transcript`.
    var assemblyAISTTModel: String = Settings.kernelDefaults.assemblyAISTTModel

    // ElevenLabs configuration
    var elevenLabsSTTModel: String = Settings.kernelDefaults.elevenLabsSTTModel
    var elevenLabsTTSModel: String = Settings.kernelDefaults.elevenLabsTTSModel
    var elevenLabsVoiceID: String = Settings.kernelDefaults.elevenLabsVoiceID
    var elevenLabsVoiceName: String = Settings.kernelDefaults.elevenLabsVoiceName

    // Playback
    /// Default playback rate (0.5x – 3.0x). Per-show overrides live on `PodcastSubscription`.
    var defaultPlaybackRate: Double = Settings.kernelDefaults.defaultPlaybackRate
    /// Seconds the forward-skip transport button advances by. Mirrored to the lock-screen.
    var skipForwardSeconds: Int = Int(Settings.kernelDefaults.skipForwardSecs)
    /// Seconds the back-skip transport button rewinds by. Mirrored to the lock-screen.
    var skipBackwardSeconds: Int = Int(Settings.kernelDefaults.skipBackwardSecs)
    /// When `true`, an episode is automatically marked played the first time playback
    /// reaches its end. Defaults on for parity with Apple Podcasts.
    var autoMarkPlayedAtEnd: Bool = Settings.kernelDefaults.autoMarkPlayedAtEnd
    /// When `true`, downloaded enclosures are deleted as soon as the episode is
    /// marked played (auto-end-of-play OR explicit "Mark as played"). Off by
    /// default — without it, downloads are kept until manually removed.
    var autoDeleteDownloadsAfterPlayed: Bool = Settings.kernelDefaults.autoDeleteDownloadsAfterPlayed
    /// When `true`, the next episode in `PlaybackState.queue` (Up Next)
    /// starts playing automatically when the current episode finishes.
    /// Defaults on for parity with Apple Podcasts. Suppressed when the
    /// sleep timer has armed an end-of-episode stop.
    var autoPlayNext: Bool = Settings.kernelDefaults.autoPlayNext
    /// When `true`, the player auto-seeks past detected ad segments
    /// (kernel `podcast.chapters.compile` output, stored on `Episode.adSegments`).
    /// Defaults on — ad detection quality is proven. The chapter rail still
    /// flags ad-overlapping chapters visually regardless.
    var autoSkipAds: Bool = Settings.kernelDefaults.autoSkipAdsEnabled
    /// Action fired by an AirPods double-tap / double-squeeze (or any headphone
    /// remote that emits `MPRemoteCommandCenter.nextTrackCommand`). Default
    /// matches the common podcast-player muscle memory: jump forward by the
    /// configured skip-forward interval.
    var headphoneDoubleTapAction: HeadphoneGestureAction =
        HeadphoneGestureAction(rawValue: Settings.kernelDefaults.headphoneDoubleTapAction) ?? .skipForward
    /// Action fired by an AirPods triple-tap / triple-squeeze (or any headphone
    /// remote that emits `MPRemoteCommandCenter.previousTrackCommand`). Default
    /// captures a clip — quickly bookmarking what you just heard is the most
    /// valuable thing a third tap can do that single/double don't already cover.
    var headphoneTripleTapAction: HeadphoneGestureAction =
        HeadphoneGestureAction(rawValue: Settings.kernelDefaults.headphoneTripleTapAction) ?? .clipNow

    // Wiki
    /// When `true`, `WikiGenerator` runs (or refreshes) the relevant wiki pages as soon as
    /// a new transcript finishes ingesting. Defaults off so first-run users don't burn
    /// tokens before deciding to opt in.
    var wikiAutoGenerateOnTranscriptIngest: Bool = Settings.kernelDefaults.wikiAutoGenerateOnTranscriptIngest

    // Transcripts
    /// When `true`, the app pre-fetches publisher-supplied transcripts in the
    /// background as soon as new episodes appear (called from
    /// `AppStateStore.upsertEpisodes` after a feed refresh). Default-on
    /// because the agent layer (RAG, wiki, summarisation) only
    /// works once the transcript exists; publisher transcripts are typically
    /// tens of KB so the bandwidth cost is small. Toggle off in
    /// Settings → Transcripts to defer everything to manual fetch.
    var autoIngestPublisherTranscripts: Bool = Settings.kernelDefaults.autoIngestPublisherTranscripts
    /// When `true`, episodes lacking a publisher transcript fall back to ElevenLabs Scribe
    /// transcription. Requires an ElevenLabs key; defaults on so existing behaviour is
    /// preserved.
    var autoFallbackToScribe: Bool = Settings.kernelDefaults.autoFallbackToScribe

    // Notifications (per-kind toggles; the system permission is separate)
    /// When `true`, fire a local notification when a feed refresh discovers a brand-new
    /// episode for a subscription that has notifications enabled.
    var notifyOnNewEpisodes: Bool = Settings.kernelDefaults.notifyOnNewEpisodes

    // Nostr identity (private key stored in Keychain via NostrCredentialStore)
    var nostrEnabled: Bool = Settings.kernelDefaults.nostrEnabled
    var nostrRelayURL: String = Settings.kernelDefaults.nostrRelayURL
    var nostrProfileName: String = Settings.kernelDefaults.nostrProfileName
    var nostrProfileAbout: String = Settings.kernelDefaults.nostrProfileAbout
    var nostrProfilePicture: String = Settings.kernelDefaults.nostrProfilePicture
    var nostrPublicKeyHex: String?

    // Onboarding
    var hasCompletedOnboarding: Bool = Settings.kernelDefaults.hasCompletedOnboarding

    init() {}

    private enum CodingKeys: String, CodingKey {
        // RawValues preserved as "llmModel" / "llmModelName" so existing
        // user data continues to decode after the rename to agentInitialModel.
        case agentInitialModel = "llmModel"
        case agentInitialModelName = "llmModelName"
        case agentThinkingModel, agentThinkingModelName
        case memoryCompilationModel, memoryCompilationModelName
        case wikiModel, wikiModelName, categorizationModel, categorizationModelName
        case chapterCompilationModel, chapterCompilationModelName
        case embeddingsModel, embeddingsModelName, rerankerEnabled
        case imageGenerationModel, imageGenerationModelName
        case blossomServerURL
        case openRouterCredentialSource
        case openRouterBYOKKeyID, openRouterBYOKKeyLabel, openRouterConnectedAt
        case ollamaCredentialSource, ollamaBYOKKeyID, ollamaBYOKKeyLabel, ollamaConnectedAt, ollamaChatURL
        case elevenLabsCredentialSource
        case elevenLabsBYOKKeyID, elevenLabsBYOKKeyLabel, elevenLabsConnectedAt
        case assemblyAICredentialSource
        case assemblyAIBYOKKeyID, assemblyAIBYOKKeyLabel, assemblyAIConnectedAt
        case perplexityCredentialSource
        case perplexityBYOKKeyID, perplexityBYOKKeyLabel, perplexityConnectedAt
        case sttProvider, openRouterWhisperModel, assemblyAISTTModel
        case elevenLabsSTTModel, elevenLabsTTSModel, elevenLabsVoiceID, elevenLabsVoiceName
        case defaultPlaybackRate, skipForwardSeconds, skipBackwardSeconds, autoMarkPlayedAtEnd
        case autoDeleteDownloadsAfterPlayed, autoPlayNext, autoSkipAds
        case headphoneDoubleTapAction, headphoneTripleTapAction
        case wikiAutoGenerateOnTranscriptIngest
        case autoIngestPublisherTranscripts, autoFallbackToScribe
        case notifyOnNewEpisodes
        case nostrEnabled, nostrRelayURL
        case nostrProfileName, nostrProfileAbout, nostrProfilePicture
        case nostrPublicKeyHex
        case hasCompletedOnboarding
        case youtubeExtractorURL
        case localModelID
    }

    init(from decoder: Decoder) throws {
        // Start from the kernel-canonical defaults (the property initializers,
        // which mirror `SettingsSnapshot()`), then overwrite only keys present
        // on the wire. No `?? literal` fallbacks: an absent key keeps the
        // canonical default set by `self.init()`.
        self.init()
        let c = try decoder.container(keyedBy: CodingKeys.self)
        if let v = try c.decodeIfPresent(String.self, forKey: .agentInitialModel) { agentInitialModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentInitialModelName) { agentInitialModelName = v }
        // Intentional semantic fallback: an absent thinking model inherits the
        // (decoded-or-default) initial model, so escalation starts from the same
        // model until the user picks something stronger.
        if let v = try c.decodeIfPresent(String.self, forKey: .agentThinkingModel) {
            agentThinkingModel = v
        } else {
            agentThinkingModel = agentInitialModel
        }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentThinkingModelName) { agentThinkingModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModel) { memoryCompilationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModelName) { memoryCompilationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .wikiModel) { wikiModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .wikiModelName) { wikiModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .categorizationModel) { categorizationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .categorizationModelName) { categorizationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModel) { chapterCompilationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModelName) { chapterCompilationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .embeddingsModel) { embeddingsModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .embeddingsModelName) { embeddingsModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .imageGenerationModel) { imageGenerationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .imageGenerationModelName) { imageGenerationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .blossomServerURL) { blossomServerURL = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .rerankerEnabled) { rerankerEnabled = v }
        if let v = try c.decodeIfPresent(OpenRouterCredentialSource.self, forKey: .openRouterCredentialSource) { openRouterCredentialSource = v }
        openRouterBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyID)
        openRouterBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyLabel)
        openRouterConnectedAt = try c.decodeIfPresent(Date.self, forKey: .openRouterConnectedAt)
        if let v = try c.decodeIfPresent(OllamaCredentialSource.self, forKey: .ollamaCredentialSource) { ollamaCredentialSource = v }
        ollamaBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyID)
        ollamaBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyLabel)
        ollamaConnectedAt = try c.decodeIfPresent(Date.self, forKey: .ollamaConnectedAt)
        if let v = try c.decodeIfPresent(String.self, forKey: .ollamaChatURL) { ollamaChatURL = v }
        if let v = try c.decodeIfPresent(ElevenLabsCredentialSource.self, forKey: .elevenLabsCredentialSource) { elevenLabsCredentialSource = v }
        elevenLabsBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyID)
        elevenLabsBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyLabel)
        elevenLabsConnectedAt = try c.decodeIfPresent(Date.self, forKey: .elevenLabsConnectedAt)
        if let v = try c.decodeIfPresent(AssemblyAICredentialSource.self, forKey: .assemblyAICredentialSource) { assemblyAICredentialSource = v }
        assemblyAIBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyID)
        assemblyAIBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyLabel)
        assemblyAIConnectedAt = try c.decodeIfPresent(Date.self, forKey: .assemblyAIConnectedAt)
        if let v = try c.decodeIfPresent(PerplexityCredentialSource.self, forKey: .perplexityCredentialSource) { perplexityCredentialSource = v }
        perplexityBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyID)
        perplexityBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyLabel)
        perplexityConnectedAt = try c.decodeIfPresent(Date.self, forKey: .perplexityConnectedAt)
        if let v = try c.decodeIfPresent(STTProvider.self, forKey: .sttProvider) { sttProvider = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .openRouterWhisperModel) { openRouterWhisperModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .assemblyAISTTModel) { assemblyAISTTModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsSTTModel) { elevenLabsSTTModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsTTSModel) { elevenLabsTTSModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceID) { elevenLabsVoiceID = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceName) { elevenLabsVoiceName = v }
        if let v = try c.decodeIfPresent(Double.self, forKey: .defaultPlaybackRate) { defaultPlaybackRate = v }
        if let v = try c.decodeIfPresent(Int.self, forKey: .skipForwardSeconds) { skipForwardSeconds = v }
        if let v = try c.decodeIfPresent(Int.self, forKey: .skipBackwardSeconds) { skipBackwardSeconds = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoMarkPlayedAtEnd) { autoMarkPlayedAtEnd = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoDeleteDownloadsAfterPlayed) { autoDeleteDownloadsAfterPlayed = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoPlayNext) { autoPlayNext = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoSkipAds) { autoSkipAds = v }
        if let v = try c.decodeIfPresent(HeadphoneGestureAction.self, forKey: .headphoneDoubleTapAction) { headphoneDoubleTapAction = v }
        if let v = try c.decodeIfPresent(HeadphoneGestureAction.self, forKey: .headphoneTripleTapAction) { headphoneTripleTapAction = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .wikiAutoGenerateOnTranscriptIngest) { wikiAutoGenerateOnTranscriptIngest = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoIngestPublisherTranscripts) { autoIngestPublisherTranscripts = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoFallbackToScribe) { autoFallbackToScribe = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .notifyOnNewEpisodes) { notifyOnNewEpisodes = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .nostrEnabled) { nostrEnabled = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrRelayURL) { nostrRelayURL = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfileName) { nostrProfileName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfileAbout) { nostrProfileAbout = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfilePicture) { nostrProfilePicture = v }
        nostrPublicKeyHex = try c.decodeIfPresent(String.self, forKey: .nostrPublicKeyHex)
        if let v = try c.decodeIfPresent(Bool.self, forKey: .hasCompletedOnboarding) { hasCompletedOnboarding = v }
        youtubeExtractorURL = try c.decodeIfPresent(String.self, forKey: .youtubeExtractorURL)
        localModelID = try c.decodeIfPresent(String.self, forKey: .localModelID)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(agentInitialModel, forKey: .agentInitialModel)
        try c.encode(agentInitialModelName, forKey: .agentInitialModelName)
        try c.encode(agentThinkingModel, forKey: .agentThinkingModel)
        try c.encode(agentThinkingModelName, forKey: .agentThinkingModelName)
        try c.encode(memoryCompilationModel, forKey: .memoryCompilationModel)
        try c.encode(memoryCompilationModelName, forKey: .memoryCompilationModelName)
        try c.encode(wikiModel, forKey: .wikiModel)
        try c.encode(wikiModelName, forKey: .wikiModelName)
        try c.encode(categorizationModel, forKey: .categorizationModel)
        try c.encode(categorizationModelName, forKey: .categorizationModelName)
        try c.encode(chapterCompilationModel, forKey: .chapterCompilationModel)
        try c.encode(chapterCompilationModelName, forKey: .chapterCompilationModelName)
        try c.encode(embeddingsModel, forKey: .embeddingsModel)
        try c.encode(embeddingsModelName, forKey: .embeddingsModelName)
        try c.encode(imageGenerationModel, forKey: .imageGenerationModel)
        try c.encode(imageGenerationModelName, forKey: .imageGenerationModelName)
        try c.encode(blossomServerURL, forKey: .blossomServerURL)
        try c.encode(rerankerEnabled, forKey: .rerankerEnabled)
        try c.encode(openRouterCredentialSource, forKey: .openRouterCredentialSource)
        try c.encodeIfPresent(openRouterBYOKKeyID, forKey: .openRouterBYOKKeyID)
        try c.encodeIfPresent(openRouterBYOKKeyLabel, forKey: .openRouterBYOKKeyLabel)
        try c.encodeIfPresent(openRouterConnectedAt, forKey: .openRouterConnectedAt)
        try c.encode(ollamaCredentialSource, forKey: .ollamaCredentialSource)
        try c.encodeIfPresent(ollamaBYOKKeyID, forKey: .ollamaBYOKKeyID)
        try c.encodeIfPresent(ollamaBYOKKeyLabel, forKey: .ollamaBYOKKeyLabel)
        try c.encodeIfPresent(ollamaConnectedAt, forKey: .ollamaConnectedAt)
        try c.encode(ollamaChatURL, forKey: .ollamaChatURL)
        try c.encode(elevenLabsCredentialSource, forKey: .elevenLabsCredentialSource)
        try c.encodeIfPresent(elevenLabsBYOKKeyID, forKey: .elevenLabsBYOKKeyID)
        try c.encodeIfPresent(elevenLabsBYOKKeyLabel, forKey: .elevenLabsBYOKKeyLabel)
        try c.encodeIfPresent(elevenLabsConnectedAt, forKey: .elevenLabsConnectedAt)
        try c.encode(assemblyAICredentialSource, forKey: .assemblyAICredentialSource)
        try c.encodeIfPresent(assemblyAIBYOKKeyID, forKey: .assemblyAIBYOKKeyID)
        try c.encodeIfPresent(assemblyAIBYOKKeyLabel, forKey: .assemblyAIBYOKKeyLabel)
        try c.encodeIfPresent(assemblyAIConnectedAt, forKey: .assemblyAIConnectedAt)
        try c.encode(perplexityCredentialSource, forKey: .perplexityCredentialSource)
        try c.encodeIfPresent(perplexityBYOKKeyID, forKey: .perplexityBYOKKeyID)
        try c.encodeIfPresent(perplexityBYOKKeyLabel, forKey: .perplexityBYOKKeyLabel)
        try c.encodeIfPresent(perplexityConnectedAt, forKey: .perplexityConnectedAt)
        try c.encode(sttProvider, forKey: .sttProvider)
        try c.encode(openRouterWhisperModel, forKey: .openRouterWhisperModel)
        try c.encode(assemblyAISTTModel, forKey: .assemblyAISTTModel)
        try c.encode(elevenLabsSTTModel, forKey: .elevenLabsSTTModel)
        try c.encode(elevenLabsTTSModel, forKey: .elevenLabsTTSModel)
        try c.encode(elevenLabsVoiceID, forKey: .elevenLabsVoiceID)
        try c.encode(elevenLabsVoiceName, forKey: .elevenLabsVoiceName)
        try c.encode(defaultPlaybackRate, forKey: .defaultPlaybackRate)
        try c.encode(skipForwardSeconds, forKey: .skipForwardSeconds)
        try c.encode(skipBackwardSeconds, forKey: .skipBackwardSeconds)
        try c.encode(autoMarkPlayedAtEnd, forKey: .autoMarkPlayedAtEnd)
        try c.encode(autoDeleteDownloadsAfterPlayed, forKey: .autoDeleteDownloadsAfterPlayed)
        try c.encode(autoPlayNext, forKey: .autoPlayNext)
        try c.encode(autoSkipAds, forKey: .autoSkipAds)
        try c.encode(headphoneDoubleTapAction, forKey: .headphoneDoubleTapAction)
        try c.encode(headphoneTripleTapAction, forKey: .headphoneTripleTapAction)
        try c.encode(wikiAutoGenerateOnTranscriptIngest, forKey: .wikiAutoGenerateOnTranscriptIngest)
        try c.encode(autoIngestPublisherTranscripts, forKey: .autoIngestPublisherTranscripts)
        try c.encode(autoFallbackToScribe, forKey: .autoFallbackToScribe)
        try c.encode(notifyOnNewEpisodes, forKey: .notifyOnNewEpisodes)
        try c.encode(nostrEnabled, forKey: .nostrEnabled)
        try c.encode(nostrRelayURL, forKey: .nostrRelayURL)
        try c.encode(nostrProfileName, forKey: .nostrProfileName)
        try c.encode(nostrProfileAbout, forKey: .nostrProfileAbout)
        try c.encode(nostrProfilePicture, forKey: .nostrProfilePicture)
        try c.encodeIfPresent(nostrPublicKeyHex, forKey: .nostrPublicKeyHex)
        try c.encode(hasCompletedOnboarding, forKey: .hasCompletedOnboarding)
        try c.encodeIfPresent(youtubeExtractorURL, forKey: .youtubeExtractorURL)
        try c.encodeIfPresent(localModelID, forKey: .localModelID)
    }

}

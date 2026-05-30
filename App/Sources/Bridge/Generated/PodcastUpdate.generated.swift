// PodcastUpdate.generated.swift
// Hand-maintained mirror of the Rust projection types until the codegen
// pipeline (`dump_projection_schemas | gen swift`) lands. Keep camelCase in
// sync with snake_case Rust source — `.convertFromSnakeCase` handles it.
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs

import Foundation

/// Top-level snapshot emitted by the Rust podcast kernel on every podcast
/// projection tick (pulled via `nmp_app_podcast_snapshot`).
struct PodcastUpdate {
    var running: Bool = false
    var rev: Int = 0
    var schemaVersion: Int = 0
    var nowPlaying: PlayerState? = nil
    var downloads: DownloadQueueSnapshot? = nil
    var agent: AgentSnapshot? = nil
    var voice: VoiceSnapshot? = nil
    var briefing: BriefingSnapshot? = nil
    var social: SocialSnapshot? = nil
    // D5: the Rust projection omits empty collections / default settings from
    // the wire. Wrap them so absent keys decode to defaults instead of throwing
    // `keyNotFound` (synthesized `Decodable` does not honor the `= []` default).
    @DefaultEmptyArray var library: [PodcastSummary] = []
    var activeAccount: AccountSummary? = nil
    var widget: WidgetSnapshot? = nil
    var toast: String? = nil
    @DefaultEmptyArray var searchResults: [PodcastSummary] = []
    @DefaultEmptyArray var nostrResults: [NostrShowSummary] = []
    @DefaultSettings var settings: SettingsSnapshot = SettingsSnapshot()
    @DefaultEmptyArray var comments: [CommentSummary] = []
    @DefaultEmptyArray var queue: [EpisodeSummary] = []
    @DefaultEmptyArray var wikiArticles: [WikiArticle] = []
    @DefaultEmptyArray var wikiSearchResults: [WikiArticle] = []
    @DefaultEmptyArray var picks: [AgentPickSummary] = []
    @DefaultEmptyArray var agentTasks: [AgentTaskSummary] = []
    @DefaultEmptyArray var knowledgeSearchResults: [KnowledgeSearchResult] = []
    @DefaultEmptyArray var memoryFacts: [MemoryFact] = []
    @DefaultEmptyArray var ttsEpisodes: [TtsEpisodeSummary] = []
    @DefaultEmptyArray var clips: [ClipSummary] = []
    @DefaultEmptyArray var inbox: [InboxItem] = []
    /// `true` while a background LLM triage pass is running. D5: omitted when false.
    @DefaultFalse var inboxTriageInProgress: Bool = false
    @DefaultEmptyArray var ownedPodcasts: [OwnedPodcastInfo] = []
    @DefaultEmptyArray var categories: [CategoryBrowseItem] = []
}

/// Active player state (present only when an episode is loaded).
struct PlayerState {
    var episodeId: String? = nil
    var podcastId: String? = nil
    var url: String? = nil
    var positionSecs: Double = 0
    var durationSecs: Double = 0
    var isPlaying: Bool = false
    var bufferingFraction: Double? = nil
    var speed: Float = 1
    var volume: Float = 1
    var sleepTimerRemainingSecs: Int? = nil
    var lastError: String? = nil
    /// Set to `true` when AVPlayer fires `AVPlayerItemDidPlayToEndTime`.
    /// Cleared when the next episode loads. Used by the UI to distinguish
    /// a natural finish from a user-initiated stop.
    var didReachNaturalEnd: Bool = false
    /// Absolute end boundary (seconds) for a bounded agent segment.
    /// Nil for unbounded playback.
    var segmentEndSecs: Double? = nil
    /// Title of the chapter active at the current playhead position.
    var currentChapterTitle: String? = nil
    /// Artwork URL of the active chapter, if the chapter has a per-chapter image.
    var currentChapterArtworkUrl: String? = nil
}

/// Active Nostr identity (present only when an account is loaded).
struct AccountSummary: Codable {
    var npub: String
    var displayName: String? = nil
    var mode: String
    var pictureUrl: String? = nil
}

/// App-settings projection. Mirrors `ffi::projections::SettingsSnapshot`.
struct SettingsSnapshot: Equatable {
    var hasCompletedOnboarding: Bool = false
    var autoSkipAdsEnabled: Bool = false
    /// When `true`, the kernel auto-advances to the next queued episode on
    /// natural episode end. Default `true`.
    var autoPlayNext: Bool = true
    /// When `true`, the kernel marks the episode listened on natural episode
    /// end. Default `true`.
    var autoMarkPlayedAtEnd: Bool = true
    /// Raw action string for headphone double-tap gesture. Default `"skipForward"`.
    var headphoneDoubleTapAction: String = "skipForward"
    /// Raw action string for headphone triple-tap gesture. Default `"clipNow"`.
    var headphoneTripleTapAction: String = "clipNow"
    /// Skip-forward interval in seconds. Default 30. Set via
    /// `podcast.settings.set_skip_intervals`.
    var skipForwardSecs: Double = 30
    /// Skip-backward interval in seconds. Default 15.
    var skipBackwardSecs: Double = 15
    /// Default playback rate. Default 1.0; range [0.5, 3.0].
    var defaultPlaybackRate: Double = 1.0
    /// When `true`, the kernel deletes the downloaded file after the episode
    /// is marked played. Default `false`.
    var autoDeleteDownloadsAfterPlayed: Bool = false
    /// LLM model ID for initial agent chat. Default `"deepseek-v4-flash:cloud"`.
    var agentInitialModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for initial agent model. Default `"DeepSeek Flash"`.
    var agentInitialModelName: String = "DeepSeek Flash"
    /// LLM model ID for agent thinking/planning. Default `"deepseek-v4-pro:cloud"`.
    var agentThinkingModel: String = "deepseek-v4-pro:cloud"
    /// Human-readable name for agent thinking model. Default `"DeepSeek Pro"`.
    var agentThinkingModelName: String = "DeepSeek Pro"
    /// LLM model ID for memory compilation. Default `"deepseek-v4-flash:cloud"`.
    var memoryCompilationModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for memory compilation model. Default `"DeepSeek Flash"`.
    var memoryCompilationModelName: String = "DeepSeek Flash"
    /// LLM model ID for wiki synthesis. Default `"deepseek-v4-flash:cloud"`.
    var wikiModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for wiki model. Default `"DeepSeek Flash"`.
    var wikiModelName: String = "DeepSeek Flash"
    /// LLM model ID for episode categorization. Default `"deepseek-v4-flash:cloud"`.
    var categorizationModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for categorization model. Default `"DeepSeek Flash"`.
    var categorizationModelName: String = "DeepSeek Flash"
    /// LLM model ID for chapter compilation. Default `"deepseek-v4-flash:cloud"`.
    var chapterCompilationModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for chapter compilation model. Default `"DeepSeek Flash"`.
    var chapterCompilationModelName: String = "DeepSeek Flash"
    /// LLM model ID for embeddings generation. Default `"deepseek-v4-flash:cloud"`.
    var embeddingsModel: String = "deepseek-v4-flash:cloud"
    /// Human-readable name for embeddings model. Default `"DeepSeek Flash"`.
    var embeddingsModelName: String = "DeepSeek Flash"
    /// LLM model ID for image generation. Default `"google/gemini-2.5-flash-image"`.
    var imageGenerationModel: String = "google/gemini-2.5-flash-image"
    /// Human-readable name for image generation model. Default `"Gemini 2.5 Flash"`.
    var imageGenerationModelName: String = "Gemini 2.5 Flash"
    /// Whether the reranker is enabled for search results. Default `false`.
    var rerankerEnabled: Bool = false
    /// OpenRouter credential source enum (raw String: "apiKey", "byok", "nostr").
    var openRouterCredentialSource: String = ""
    /// OpenRouter BYOK key ID (optional).
    var openRouterBYOKKeyID: String? = nil
    /// OpenRouter BYOK key label (optional).
    var openRouterBYOKKeyLabel: String? = nil
    /// OpenRouter credential connected-at timestamp (optional, converted to Date in Swift).
    var openRouterConnectedAt: Date? = nil
    /// Ollama credential source enum (raw String: "apiKey", "byok", "nostr").
    var ollamaCredentialSource: String = ""
    /// Ollama BYOK key ID (optional).
    var ollamaBYOKKeyID: String? = nil
    /// Ollama BYOK key label (optional).
    var ollamaBYOKKeyLabel: String? = nil
    /// Ollama credential connected-at timestamp (optional, converted to Date in Swift).
    var ollamaConnectedAt: Date? = nil
    /// Ollama chat endpoint URL for LLM inference.
    var ollamaChatURL: String = ""
    /// ElevenLabs credential source enum (raw String: "apiKey", "byok", "nostr").
    var elevenLabsCredentialSource: String = ""
    /// ElevenLabs BYOK key ID (optional).
    var elevenLabsBYOKKeyID: String? = nil
    /// ElevenLabs BYOK key label (optional).
    var elevenLabsBYOKKeyLabel: String? = nil
    /// ElevenLabs credential connected-at timestamp (optional, converted to Date in Swift).
    var elevenLabsConnectedAt: Date? = nil
    /// STT provider selection enum (raw String: "elevenlabs_scribe", etc).
    var sttProvider: String = "elevenlabs_scribe"
    /// OpenRouter Whisper model string. Default `"openai/whisper-1"`.
    var openRouterWhisperModel: String = "openai/whisper-1"
    /// AssemblyAI STT model string. Default `"universal-3-pro,universal-2"`.
    var assemblyAISTTModel: String = "universal-3-pro,universal-2"
    /// ElevenLabs STT model string. Default `"scribe_v1"`.
    var elevenLabsSTTModel: String = "scribe_v1"
    /// ElevenLabs TTS model string. Default `"eleven_turbo_v2_5"`.
    var elevenLabsTTSModel: String = "eleven_turbo_v2_5"
    /// ElevenLabs voice ID. Defaults to empty string.
    var elevenLabsVoiceID: String = ""
    /// ElevenLabs voice name. Defaults to empty string.
    var elevenLabsVoiceName: String = ""
    /// Blossom server URL. Default `"https://blossom.primal.net"`.
    var blossomServerURL: String = "https://blossom.primal.net"
    /// YouTube extractor URL (optional).
    var youtubeExtractorURL: String? = nil
}

/// Active download-queue projection surfaced via `PodcastUpdate.downloads`.
struct DownloadQueueSnapshot: Equatable {
    var active: [DownloadItemSnapshot] = []
    var queuedCount: Int = 0
    var completedToday: Int = 0
}

/// One row in `DownloadQueueSnapshot.active`.
struct DownloadItemSnapshot: Identifiable, Equatable {
    var episodeId: String
    var progress: Double = 0
    var state: String
    /// Total file size (bytes) once the server reports `Content-Length`.
    /// `nil` until the first HTTP response arrives.
    var totalBytes: Int64? = nil
    var error: String? = nil

    var id: String { episodeId }
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false), Vec fields (omit when empty), and `settings` (omit when default).
// Swift's synthesized Decodable requires every non-optional key to be present,
// but these keys are legitimately absent from snapshots where the value is the
// zero/default. Custom `init(from:)` in extensions uses `decodeIfPresent` with
// explicit fallbacks so the decoder is forward- and backward-compatible.
//
// WHY extensions, not struct bodies: putting `init(from:)` inside the struct
// body suppresses the synthesized memberwise init. Extensions preserve it.

extension PodcastUpdate: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        running = try c.decodeIfPresent(Bool.self, forKey: .running) ?? false
        rev = try c.decodeIfPresent(Int.self, forKey: .rev) ?? 0
        schemaVersion = try c.decodeIfPresent(Int.self, forKey: .schemaVersion) ?? 0
        nowPlaying = try c.decodeIfPresent(PlayerState.self, forKey: .nowPlaying)
        downloads = try c.decodeIfPresent(DownloadQueueSnapshot.self, forKey: .downloads)
        agent = try c.decodeIfPresent(AgentSnapshot.self, forKey: .agent)
        voice = try c.decodeIfPresent(VoiceSnapshot.self, forKey: .voice)
        briefing = try c.decodeIfPresent(BriefingSnapshot.self, forKey: .briefing)
        social = try c.decodeIfPresent(SocialSnapshot.self, forKey: .social)
        library = try c.decodeIfPresent([PodcastSummary].self, forKey: .library) ?? []
        activeAccount = try c.decodeIfPresent(AccountSummary.self, forKey: .activeAccount)
        widget = try c.decodeIfPresent(WidgetSnapshot.self, forKey: .widget)
        toast = try c.decodeIfPresent(String.self, forKey: .toast)
        searchResults = try c.decodeIfPresent([PodcastSummary].self, forKey: .searchResults) ?? []
        nostrResults = try c.decodeIfPresent([NostrShowSummary].self, forKey: .nostrResults) ?? []
        settings = try c.decodeIfPresent(SettingsSnapshot.self, forKey: .settings) ?? SettingsSnapshot()
        comments = try c.decodeIfPresent([CommentSummary].self, forKey: .comments) ?? []
        queue = try c.decodeIfPresent([EpisodeSummary].self, forKey: .queue) ?? []
        wikiArticles = try c.decodeIfPresent([WikiArticle].self, forKey: .wikiArticles) ?? []
        wikiSearchResults = try c.decodeIfPresent([WikiArticle].self, forKey: .wikiSearchResults) ?? []
        picks = try c.decodeIfPresent([AgentPickSummary].self, forKey: .picks) ?? []
        agentTasks = try c.decodeIfPresent([AgentTaskSummary].self, forKey: .agentTasks) ?? []
        knowledgeSearchResults = try c.decodeIfPresent([KnowledgeSearchResult].self, forKey: .knowledgeSearchResults) ?? []
        memoryFacts = try c.decodeIfPresent([MemoryFact].self, forKey: .memoryFacts) ?? []
        ttsEpisodes = try c.decodeIfPresent([TtsEpisodeSummary].self, forKey: .ttsEpisodes) ?? []
        clips = try c.decodeIfPresent([ClipSummary].self, forKey: .clips) ?? []
        inbox = try c.decodeIfPresent([InboxItem].self, forKey: .inbox) ?? []
        inboxTriageInProgress = try c.decodeIfPresent(Bool.self, forKey: .inboxTriageInProgress) ?? false
        ownedPodcasts = try c.decodeIfPresent([OwnedPodcastInfo].self, forKey: .ownedPodcasts) ?? []
        categories = try c.decodeIfPresent([CategoryBrowseItem].self, forKey: .categories) ?? []
    }
}

extension PlayerState: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decodeIfPresent(String.self, forKey: .episodeId)
        podcastId = try c.decodeIfPresent(String.self, forKey: .podcastId)
        url = try c.decodeIfPresent(String.self, forKey: .url)
        positionSecs = try c.decodeIfPresent(Double.self, forKey: .positionSecs) ?? 0
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs) ?? 0
        isPlaying = try c.decodeIfPresent(Bool.self, forKey: .isPlaying) ?? false
        bufferingFraction = try c.decodeIfPresent(Double.self, forKey: .bufferingFraction)
        speed = try c.decodeIfPresent(Float.self, forKey: .speed) ?? 1
        volume = try c.decodeIfPresent(Float.self, forKey: .volume) ?? 1
        sleepTimerRemainingSecs = try c.decodeIfPresent(Int.self, forKey: .sleepTimerRemainingSecs)
        lastError = try c.decodeIfPresent(String.self, forKey: .lastError)
        didReachNaturalEnd = try c.decodeIfPresent(Bool.self, forKey: .didReachNaturalEnd) ?? false
        segmentEndSecs = try c.decodeIfPresent(Double.self, forKey: .segmentEndSecs)
        currentChapterTitle = try c.decodeIfPresent(String.self, forKey: .currentChapterTitle)
        currentChapterArtworkUrl = try c.decodeIfPresent(String.self, forKey: .currentChapterArtworkUrl)
    }
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
        case openRouterBYOKKeyID = "open_router_byok_key_id"
        case openRouterBYOKKeyLabel = "open_router_byok_key_label"
        case openRouterConnectedAt
        case ollamaCredentialSource
        case ollamaBYOKKeyID = "ollama_byok_key_id"
        case ollamaBYOKKeyLabel = "ollama_byok_key_label"
        case ollamaConnectedAt
        case ollamaChatURL = "ollama_chat_url"
        case elevenLabsCredentialSource
        case elevenLabsBYOKKeyID = "eleven_labs_byok_key_id"
        case elevenLabsBYOKKeyLabel = "eleven_labs_byok_key_label"
        case elevenLabsConnectedAt
        case sttProvider = "stt_provider"
        case openRouterWhisperModel = "open_router_whisper_model"
        case assemblyAISTTModel = "assembly_ai_stt_model"
        case elevenLabsSTTModel = "eleven_labs_stt_model"
        case elevenLabsTTSModel = "eleven_labs_tts_model"
        case elevenLabsVoiceID = "eleven_labs_voice_id"
        case elevenLabsVoiceName = "eleven_labs_voice_name"
        case blossomServerURL = "blossom_server_url"
        case youtubeExtractorURL = "youtube_extractor_url"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        hasCompletedOnboarding = try c.decodeIfPresent(Bool.self, forKey: .hasCompletedOnboarding) ?? false
        autoSkipAdsEnabled = try c.decodeIfPresent(Bool.self, forKey: .autoSkipAdsEnabled) ?? false
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
        openRouterCredentialSource = try c.decodeIfPresent(String.self, forKey: .openRouterCredentialSource) ?? ""
        openRouterBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyID)
        openRouterBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyLabel)
        if let timestamp = try c.decodeIfPresent(Int.self, forKey: .openRouterConnectedAt) {
            openRouterConnectedAt = Date(timeIntervalSince1970: TimeInterval(timestamp))
        }
        ollamaCredentialSource = try c.decodeIfPresent(String.self, forKey: .ollamaCredentialSource) ?? ""
        ollamaBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyID)
        ollamaBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyLabel)
        if let timestamp = try c.decodeIfPresent(Int.self, forKey: .ollamaConnectedAt) {
            ollamaConnectedAt = Date(timeIntervalSince1970: TimeInterval(timestamp))
        }
        ollamaChatURL = try c.decodeIfPresent(String.self, forKey: .ollamaChatURL) ?? ""
        elevenLabsCredentialSource = try c.decodeIfPresent(String.self, forKey: .elevenLabsCredentialSource) ?? ""
        elevenLabsBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyID)
        elevenLabsBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyLabel)
        if let timestamp = try c.decodeIfPresent(Int.self, forKey: .elevenLabsConnectedAt) {
            elevenLabsConnectedAt = Date(timeIntervalSince1970: TimeInterval(timestamp))
        }
        sttProvider = try c.decodeIfPresent(String.self, forKey: .sttProvider) ?? "elevenlabs_scribe"
        openRouterWhisperModel = try c.decodeIfPresent(String.self, forKey: .openRouterWhisperModel) ?? "openai/whisper-1"
        assemblyAISTTModel = try c.decodeIfPresent(String.self, forKey: .assemblyAISTTModel) ?? "universal-3-pro,universal-2"
        elevenLabsSTTModel = try c.decodeIfPresent(String.self, forKey: .elevenLabsSTTModel) ?? "scribe_v1"
        elevenLabsTTSModel = try c.decodeIfPresent(String.self, forKey: .elevenLabsTTSModel) ?? "eleven_turbo_v2_5"
        elevenLabsVoiceID = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceID) ?? ""
        elevenLabsVoiceName = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceName) ?? ""
        blossomServerURL = try c.decodeIfPresent(String.self, forKey: .blossomServerURL) ?? "https://blossom.primal.net"
        youtubeExtractorURL = try c.decodeIfPresent(String.self, forKey: .youtubeExtractorURL)
    }
}

extension DownloadQueueSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        active = try c.decodeIfPresent([DownloadItemSnapshot].self, forKey: .active) ?? []
        queuedCount = try c.decodeIfPresent(Int.self, forKey: .queuedCount) ?? 0
        completedToday = try c.decodeIfPresent(Int.self, forKey: .completedToday) ?? 0
    }
}

extension DownloadItemSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        progress = try c.decodeIfPresent(Double.self, forKey: .progress) ?? 0
        state = try c.decode(String.self, forKey: .state)
        totalBytes = try c.decodeIfPresent(Int64.self, forKey: .totalBytes)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}

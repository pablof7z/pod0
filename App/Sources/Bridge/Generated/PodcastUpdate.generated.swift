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

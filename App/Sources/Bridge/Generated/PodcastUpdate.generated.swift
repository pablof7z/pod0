// PodcastUpdate.generated.swift
// Hand-maintained mirror of the Rust projection types until the codegen
// pipeline (`dump_projection_schemas | gen swift`) lands. Keep camelCase in
// sync with snake_case Rust source — `.convertFromSnakeCase` handles it.
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs

import Foundation

/// Top-level snapshot emitted by the Rust podcast kernel on every podcast
/// projection tick (pulled via `nmp_app_podcast_snapshot`).
struct PodcastUpdate: Codable {
    var running: Bool = false
    var rev: Int = 0
    var schemaVersion: Int = 0
    var nowPlaying: PlayerState? = nil
    var downloads: DownloadQueueSnapshot? = nil
    var agent: AgentSnapshot? = nil
    var voice: VoiceSnapshot? = nil
    var briefing: BriefingSnapshot? = nil
    var social: SocialSnapshot? = nil
    var library: [PodcastSummary] = []
    var activeAccount: AccountSummary? = nil
    var widget: WidgetSnapshot? = nil
    var toast: String? = nil
    var searchResults: [PodcastSummary] = []
    var nostrResults: [NostrShowSummary] = []
    var settings: SettingsSnapshot = SettingsSnapshot()
    var comments: [CommentSummary] = []
    var queue: [EpisodeSummary] = []
    var wikiArticles: [WikiArticle] = []
    var wikiSearchResults: [WikiArticle] = []
    var picks: [AgentPickSummary] = []
    var agentTasks: [AgentTaskSummary] = []
    var knowledgeSearchResults: [KnowledgeSearchResult] = []
    var memoryFacts: [MemoryFact] = []
    var ttsEpisodes: [TtsEpisodeSummary] = []
    var clips: [ClipSummary] = []
    var inbox: [InboxItem] = []
    var ownedPodcasts: [OwnedPodcastInfo] = []
    var categories: [CategoryBrowseItem] = []
}

/// Active player state (present only when an episode is loaded).
struct PlayerState: Codable {
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
struct SettingsSnapshot: Codable, Equatable {
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
}

/// Active download-queue projection surfaced via `PodcastUpdate.downloads`.
struct DownloadQueueSnapshot: Codable, Equatable {
    var active: [DownloadItemSnapshot] = []
    var queuedCount: Int = 0
    var completedToday: Int = 0
}

/// One row in `DownloadQueueSnapshot.active`.
struct DownloadItemSnapshot: Codable, Identifiable, Equatable {
    var episodeId: String
    var progress: Double = 0
    var state: String
    var error: String? = nil

    var id: String { episodeId }
}

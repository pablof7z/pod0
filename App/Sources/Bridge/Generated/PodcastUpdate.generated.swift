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
    /// Agent-prompt inventory context: kernel-owned selection/ordering/capping
    /// of the subscribed-show list, in-progress episodes, and recent-unplayed
    /// episodes the `AgentPrompt` builder renders into its system prompt.
    /// `nil` when the library is empty.
    var agentContext: AgentContextSnapshot? = nil
    var voice: VoiceSnapshot? = nil
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
    @DefaultEmptyArray var picks: [AgentPickSummary] = []
    @DefaultEmptyArray var agentTasks: [AgentTaskSummary] = []
    @DefaultEmptyArray var knowledgeSearchResults: [KnowledgeSearchResult] = []
    @DefaultEmptyArray var memoryFacts: [MemoryFact] = []
    @DefaultEmptyArray var ttsEpisodes: [TtsEpisodeSummary] = []
    @DefaultEmptyArray var clips: [ClipSummary] = []
    @DefaultEmptyArray var inbox: [InboxItem] = []
    /// `true` while a background LLM triage pass is running. D5: omitted when false.
    @DefaultFalse var inboxTriageInProgress: Bool = false
    /// Unix seconds for the most recent completed inbox triage pass.
    var inboxLastTriagedAt: Int? = nil
    @DefaultEmptyArray var ownedPodcasts: [OwnedPodcastInfo] = []
    @DefaultEmptyArray var categories: [CategoryBrowseItem] = []
    /// NIP-10-threaded Nostr conversations (inbound + outbound merged), newest-first
    /// by lastActivity. Subsumes the retired flat `agent_notes` list.
    /// Empty until the first `FetchAgentNotes` or outbound auto-reply.
    @DefaultEmptyArray var nostrConversations: [NostrConversationDTO] = []
    /// User-configured app relays (NMP v0.2.1 `configured_relays`). Each row
    /// carries the relay URL plus its NIP-65 role string. Drives the App
    /// Relays editor. Empty until the kernel seeds defaults at start or the
    /// user adds a relay.
    @DefaultEmptyArray var configuredRelays: [AppRelayRow] = []
    /// In-app feedback events (TENEX project notes): kind:1 messages/replies +
    /// kind:513 metadata, all bearing the project `["a"]` coord. Each row is a
    /// `SignedNostrEvent`-shaped object (`pubkey` is the author, `sig` is empty).
    /// Empty until the first `fetch_feedback` dispatch. `FeedbackStore` rebuilds
    /// threads from this flat list (replacing the deleted `FeedbackRelayClient`
    /// WebSocket fetch). Decoded into `FeedbackEventDTO` — NOT `SignedNostrEvent`
    /// — because the snapshot decoder runs `.convertFromSnakeCase`, which would
    /// rename `created_at` and break `SignedNostrEvent`'s explicit coding key.
    @DefaultEmptyArray var feedbackEvents: [FeedbackEventDTO] = []
    /// Resolved feedback threads (#354): the kernel performs the NIP-10
    /// reduction + newest-wins kind:513 metadata; `FeedbackStore` renders these
    /// directly instead of rebuilding threads from `feedbackEvents`.
    @DefaultEmptyArray var feedbackThreads: [FeedbackThreadDTO] = []
}

/// Snapshot-decode mirror of a raw feedback Nostr event, retained for the
/// `FeedbackStore` loading-state check ("has any feedback event arrived?").
/// Thread reduction now happens kernel-side (#354) — see `feedbackThreads` /
/// `FeedbackThreadDTO`. camelCase `createdAt` survives `.convertFromSnakeCase`.
struct FeedbackEventDTO: Codable, Equatable {
    var id: String = ""
    var pubkey: String = ""
    var createdAt: Int = 0
    var kind: Int = 0
    var tags: [[String]] = []
    var content: String = ""

}

/// Snapshot-decode mirror of the kernel's resolved feedback thread (#354).
/// Mirrors `nmp_feedback::projection::FeedbackThreadDto` (snake_case fields
/// survive the decoder's `.convertFromSnakeCase`). The kernel owns the Nostr
/// reduction; the shell maps this to its view `FeedbackThread`.
struct FeedbackThreadDTO: Codable, Equatable {
    var eventId: String = ""
    var authorPubkey: String = ""
    var category: String = ""
    var content: String = ""
    var createdAt: Int = 0
    var title: String? = nil
    var summary: String? = nil
    var statusLabel: String? = nil
    var replies: [FeedbackReplyDTO] = []
}

/// Snapshot-decode mirror of a resolved feedback reply (#354).
/// Mirrors `nmp_feedback::projection::FeedbackReplyDto`.
struct FeedbackReplyDTO: Codable, Equatable {
    var eventId: String = ""
    var authorPubkey: String = ""
    var content: String = ""
    var createdAt: Int = 0
}

/// One configured app relay: URL plus NIP-65 role string
/// (`read` | `write` | `both` | `indexer`, optionally comma-joined).
/// Mirrors `ffi::snapshot_update::AppRelayRow`.
struct AppRelayRow: Codable, Equatable, Identifiable {
    var url: String = ""
    var role: String = ""
    var id: String { url }
}

// AgentContextSnapshot / AgentContextEpisode live in
// `PodcastAgentContextTypes.generated.swift` to keep this file under the
// 500-line hard limit.

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
    var sleepTimerEndOfEpisode: Bool = false
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
    /// Lowercase 64-hex pubkey. This is the canonical account id; `npub` is
    /// for display.
    var pubkeyHex: String
    /// Short stable account fingerprint derived by Rust from SHA-256(pubkey bytes).
    var fingerprint: String? = nil
    var displayName: String? = nil
    var mode: String
    var pictureUrl: String? = nil
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
        agentContext = try c.decodeIfPresent(AgentContextSnapshot.self, forKey: .agentContext)
        voice = try c.decodeIfPresent(VoiceSnapshot.self, forKey: .voice)
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
        picks = try c.decodeIfPresent([AgentPickSummary].self, forKey: .picks) ?? []
        agentTasks = try c.decodeIfPresent([AgentTaskSummary].self, forKey: .agentTasks) ?? []
        knowledgeSearchResults = try c.decodeIfPresent([KnowledgeSearchResult].self, forKey: .knowledgeSearchResults) ?? []
        memoryFacts = try c.decodeIfPresent([MemoryFact].self, forKey: .memoryFacts) ?? []
        ttsEpisodes = try c.decodeIfPresent([TtsEpisodeSummary].self, forKey: .ttsEpisodes) ?? []
        clips = try c.decodeIfPresent([ClipSummary].self, forKey: .clips) ?? []
        inbox = try c.decodeIfPresent([InboxItem].self, forKey: .inbox) ?? []
        inboxTriageInProgress = try c.decodeIfPresent(Bool.self, forKey: .inboxTriageInProgress) ?? false
        inboxLastTriagedAt = try c.decodeIfPresent(Int.self, forKey: .inboxLastTriagedAt)
        ownedPodcasts = try c.decodeIfPresent([OwnedPodcastInfo].self, forKey: .ownedPodcasts) ?? []
        categories = try c.decodeIfPresent([CategoryBrowseItem].self, forKey: .categories) ?? []
        nostrConversations = try c.decodeIfPresent([NostrConversationDTO].self, forKey: .nostrConversations) ?? []
        configuredRelays = try c.decodeIfPresent([AppRelayRow].self, forKey: .configuredRelays) ?? []
        feedbackEvents = try c.decodeIfPresent([FeedbackEventDTO].self, forKey: .feedbackEvents) ?? []
        feedbackThreads = try c.decodeIfPresent([FeedbackThreadDTO].self, forKey: .feedbackThreads) ?? []
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
        sleepTimerEndOfEpisode = try c.decodeIfPresent(Bool.self, forKey: .sleepTimerEndOfEpisode) ?? false
        lastError = try c.decodeIfPresent(String.self, forKey: .lastError)
        didReachNaturalEnd = try c.decodeIfPresent(Bool.self, forKey: .didReachNaturalEnd) ?? false
        segmentEndSecs = try c.decodeIfPresent(Double.self, forKey: .segmentEndSecs)
        currentChapterTitle = try c.decodeIfPresent(String.self, forKey: .currentChapterTitle)
        currentChapterArtworkUrl = try c.decodeIfPresent(String.self, forKey: .currentChapterArtworkUrl)
    }
}

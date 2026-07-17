import Foundation

// MARK: - PodcastAgentToolDeps
//
// This file defines the dependency surface that the lane-10 podcast tools
// dispatch into. Every protocol here is intentionally **value-typed in / value-typed
// out** so the caller can supply mocks in tests and lane-1..9 implementations at
// merge time.
//
// Lane 10 owns the tool surface; other lanes own the protocol implementations.
// At wire-up time the orchestrator constructs a single `PodcastAgentToolDeps`
// (typically in `AppStateStore` or an `AgentChatSession` factory) and passes it
// to `AgentTools.dispatchPodcast(...)`.
//
// All protocols are declared `Sendable` because the dispatch is `async` and may
// hop actors. Implementations that touch `@MainActor` state should mark their
// methods `@MainActor`; the protocol surface tolerates either.
//
// Value-type result envelopes (`EpisodeHit`, etc.) live in
// `PodcastAgentToolValues.swift`.

// MARK: - Search & retrieval

/// RAG search across transcripts (lane 4).
public protocol PodcastAgentRAGSearchProtocol: Sendable {
    /// Semantic + keyword episode discovery. `scope` is an optional podcast ID
    /// to constrain the search. Limit defaults to 10.
    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit]

    /// Semantic transcript chunk lookup. `scope` may be an `EpisodeID` (single
    /// episode), a `PodcastID` (whole podcast), or `nil` (everything).
    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit]

    /// Find episodes semantically similar to a seed episode.
    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit]
}

// MARK: - Composer / summarizer / fetcher

/// Summarization for an individual episode (lane 5/8).
public protocol EpisodeSummarizerProtocol: Sendable {
    func summarizeEpisode(episodeID: EpisodeID, length: String?) async throws -> EpisodeSummary
}

/// Episode metadata + existence check (lane 2/3).
public protocol EpisodeFetcherProtocol: Sendable {
    /// Returns `true` iff an episode with the given ID exists in the local
    /// library (any Podcast row, subscribed or not). Used by `play_episode`
    /// to validate before touching the player.
    func episodeExists(episodeID: EpisodeID) async -> Bool

    /// Returns `(podcastTitle, episodeTitle, durationSeconds?)` for an episode,
    /// or nil if not found. Best-effort metadata for tool result envelopes.
    func episodeMetadata(episodeID: EpisodeID) async -> (podcastTitle: String, episodeTitle: String, durationSeconds: Int?)?

    /// Scan a subscribed podcast's episodes and return the EpisodeID whose
    /// `enclosureURL` matches `audioURLString`. Returns `nil` when not found.
    /// Used by `download_and_transcribe` (external path) to locate an episode
    /// after an auto-subscribe.
    func episodeIDForAudioURL(_ audioURLString: String, podcastID: PodcastID) async -> EpisodeID?
}

// MARK: - Player / library / peer publishing

/// Player + UI host (lane 1/2/9). The agent uses this to mutate what the user
/// sees and hears.
public protocol PlaybackHostProtocol: Sendable {
    /// Play an episode already in the store. `startSeconds` defaults to 0;
    /// `endSeconds` bounds the segment when set. `queuePosition` controls
    /// whether playback starts immediately (`.now`) or the item is queued
    /// without interrupting current playback (`.next`, `.end`).
    func playEpisode(
        episodeID: EpisodeID,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult?

    /// Pause active playback and flush persisted position state.
    /// Returns `true` when the command was applied; `false` when no active
    /// playback host was available to receive it.
    func pausePlayback() async -> Bool

    /// Set playback rate. Returns the clamped rate that was applied, or `nil`
    /// when no active playback host was available.
    func setPlaybackRate(_ rate: Double) async -> Double?

    /// Arm or clear the sleep timer. Returns a human-readable label for the
    /// active timer mode, or `nil` when no active playback host was available.
    func setSleepTimer(mode: String, minutes: Int?) async -> String?

    /// Play a publicly-accessible episode by URL without requiring a prior
    /// subscription. Captures the episode (and optional source podcast) into
    /// the store, then routes through the same queue plumbing as
    /// `playEpisode`. `startSeconds` / `endSeconds` mirror the library
    /// variant — pass them to seek to a position or play a bounded segment.
    /// When `feedURLString` is supplied, the system enriches the parent
    /// podcast's metadata (artwork, title, author) in the background; when
    /// nil, the episode parents to the built-in "Unknown" podcast row.
    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult?
}

/// Library, transcript, feed, and local episode-state mutations.
public protocol PodcastLibraryProtocol: Sendable {
    func markEpisodePlayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult
    func markEpisodeUnplayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult
    func downloadEpisode(episodeID: EpisodeID) async throws -> EpisodeMutationResult
    func requestTranscription(episodeID: EpisodeID) async throws -> TranscriptRequestResult
    /// Start the download (for offline) and **await** the full transcription pipeline.
    /// Blocks until the transcript reaches `.ready` or `.failed` — use this when the
    /// agent must have the transcript available before proceeding (e.g. `query_transcripts`).
    func downloadAndTranscribe(episodeID: EpisodeID) async throws -> TranscriptRequestResult
    func refreshFeed(podcastID: PodcastID) async throws -> FeedRefreshResult
    /// Create a clip on behalf of the user. `transcriptText` is pre-filled when
    /// the agent already has it from a prior `query_transcripts` call; otherwise
    /// the implementation should attempt to extract it from the local transcript.
    func createClip(
        episodeID: EpisodeID,
        startSeconds: Double,
        endSeconds: Double,
        caption: String?,
        transcriptText: String?
    ) async throws -> ClipResult
}

// MARK: - Inventory queries

/// Plain-English library inventory queries. None of these go through RAG —
/// the agent uses them to answer "what am I subscribed to?" or "what was I
/// listening to?" without spending a search budget. Detail / discovery /
/// content lookups still go through the search protocol.
public protocol PodcastInventoryProtocol: Sendable {
    /// Every show the user is currently subscribed to, sorted by title. Caps
    /// at `limit` if the library is huge.
    func listSubscriptions(limit: Int) async -> [SubscriptionSummary]

    /// Every podcast known to the store, sorted by title — subscribed AND
    /// unsubscribed (e.g. one-off external plays, captured-via-browse feeds,
    /// the AI-generated show). Each row carries a `subscribed` flag so the
    /// agent can distinguish follow state. Mirrors the All Podcasts UI.
    func listPodcasts(limit: Int) async -> [PodcastInventoryRow]

    /// Episodes belonging to a specific podcast, newest publish-date
    /// first. Returns `nil` if the podcast isn't in the user's library.
    func listEpisodes(podcastID: PodcastID, limit: Int) async -> [EpisodeInventoryRow]?

    /// Episodes the user has started but not finished, newest publish-date
    /// first. Drives "what was I listening to?" answers without semantic
    /// search.
    func listInProgress(limit: Int) async -> [EpisodeInventoryRow]

    /// Recently published episodes the user has not played, newest first.
    /// Mirrors what the Today tab's New Episodes feed shows the user.
    func listRecentUnplayed(limit: Int) async -> [EpisodeInventoryRow]
}

/// LLM-derived category inventory and membership mutations.
public protocol PodcastCategoryProtocol: Sendable {
    /// Categories generated for the user's library. `includePodcasts` controls
    /// whether each category carries compact show rows or only counts.
    func listCategories(limit: Int, includePodcasts: Bool) async -> [PodcastCategorySummary]

    /// Move a subscribed podcast into an existing generated category.
    func changePodcastCategory(
        podcastID: PodcastID,
        category: PodcastCategoryReference
    ) async throws -> PodcastCategoryChangeResult
}

/// HTTP-bearing online lookup (lane 9).
public protocol PerplexityClientProtocol: Sendable {
    /// Run an online search. May throw on transport errors, missing API key,
    /// or rate limits.
    func search(query: String) async throws -> PerplexityResult
}

/// TTS episode generation and voice configuration (lane 10).
protocol TTSPublisherProtocol: Sendable {
    func defaultVoiceID() -> String
    func setDefaultVoiceID(_ voiceID: String)
    func generateAndPublish(
        title: String,
        description: String?,
        turns: [TTSTurn],
        playNow: Bool,
        generationSource: Episode.GenerationSource?,
        targetPodcastID: UUID?
    ) async throws -> TTSEpisodeResult
}

/// Global podcast directory search (iTunes Search API).
public protocol PodcastDirectoryProtocol: Sendable {
    /// Search for shows or episodes in the Apple Podcasts directory.
    /// `type` selects podcast-level or episode-level results.
    func searchDirectory(
        query: String,
        type: PodcastDirectorySearchType,
        limit: Int
    ) async throws -> [PodcastDirectoryHit]

    /// Resolve an iTunes collection ID (the numeric string the directory
    /// returns alongside each podcast hit) to the canonical RSS feed URL.
    /// Returns `nil` when the lookup endpoint has no row for the ID.
    /// Throws on transport / parse failure.
    func lookupFeedURL(forCollectionID collectionID: String) async throws -> String?
}

/// Subscribing to a new podcast feed by URL, plus the destructive inverse
/// (delete a podcast and everything tied to it).
public protocol PodcastSubscribeProtocol: Sendable {
    /// Fetch and persist a podcast feed and add a `PodcastSubscription` row
    /// for it. Idempotent — if the user is already subscribed the result
    /// carries `alreadySubscribed: true`.
    func subscribe(feedURLString: String) async throws -> PodcastSubscribeResult

    /// Capture a podcast's metadata + episodes into the store WITHOUT
    /// creating a `PodcastSubscription` (no subscribe). Wraps
    /// `SubscriptionService.ensurePodcast(feedURLString:)`. Used by the
    /// `list_episodes` external-input paths so the agent can offer episode
    /// lists for shows the user has not subscribed to.
    func ensurePodcast(feedURLString: String) async throws -> PodcastEnsureResult

    /// Fully delete a podcast: removes the `Podcast` row, any
    /// `PodcastSubscription` for it, and every episode tied to it. Used by
    /// the `delete_podcast` agent tool. Idempotent — succeeds with a zero
    /// episode count when the podcast is already gone.
    func deletePodcast(podcastID: PodcastID) async throws -> PodcastDeleteResult
}

// MARK: - Aggregate

/// Bundle of every protocol the podcast tool surface needs. Construct once at
/// app startup; pass to `AgentTools.dispatchPodcast(...)` for every tool call.
struct PodcastAgentToolDeps: Sendable {
    let rag: PodcastAgentRAGSearchProtocol
    let summarizer: EpisodeSummarizerProtocol
    let fetcher: EpisodeFetcherProtocol
    let playback: PlaybackHostProtocol
    let library: PodcastLibraryProtocol
    let inventory: PodcastInventoryProtocol
    let categories: PodcastCategoryProtocol
    let perplexity: PerplexityClientProtocol
    let ttsPublisher: TTSPublisherProtocol
    let directory: PodcastDirectoryProtocol
    let subscribe: PodcastSubscribeProtocol
    let youtubeIngestion: YouTubeIngestionProtocol
    let ownedPodcasts: AgentOwnedPodcastManagerProtocol
    /// Set by `AgentChatSession` per dispatch to the active in-app conversation
    /// UUID. Used by `generate_tts_episode` to tag the resulting episode with
    /// its source conversation so the player can surface a tappable link.
    let chatConversationID: UUID?
    init(
        rag: PodcastAgentRAGSearchProtocol,
        summarizer: EpisodeSummarizerProtocol,
        fetcher: EpisodeFetcherProtocol,
        playback: PlaybackHostProtocol,
        library: PodcastLibraryProtocol,
        inventory: PodcastInventoryProtocol,
        categories: PodcastCategoryProtocol,
        perplexity: PerplexityClientProtocol,
        ttsPublisher: TTSPublisherProtocol,
        directory: PodcastDirectoryProtocol,
        subscribe: PodcastSubscribeProtocol,
        youtubeIngestion: YouTubeIngestionProtocol,
        ownedPodcasts: AgentOwnedPodcastManagerProtocol,
        chatConversationID: UUID? = nil
    ) {
        self.rag = rag
        self.summarizer = summarizer
        self.fetcher = fetcher
        self.playback = playback
        self.library = library
        self.inventory = inventory
        self.categories = categories
        self.perplexity = perplexity
        self.ttsPublisher = ttsPublisher
        self.directory = directory
        self.subscribe = subscribe
        self.youtubeIngestion = youtubeIngestion
        self.ownedPodcasts = ownedPodcasts
        self.chatConversationID = chatConversationID
    }

    func withChatConversationID(_ id: UUID?) -> PodcastAgentToolDeps {
        PodcastAgentToolDeps(
            rag: rag, summarizer: summarizer,
            fetcher: fetcher, playback: playback, library: library,
            inventory: inventory, categories: categories,
            perplexity: perplexity,
            ttsPublisher: ttsPublisher, directory: directory,
            subscribe: subscribe, youtubeIngestion: youtubeIngestion,
            ownedPodcasts: ownedPodcasts,
            chatConversationID: id
        )
    }
}


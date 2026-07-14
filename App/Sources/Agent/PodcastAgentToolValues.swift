import Foundation

// MARK: - PodcastAgentToolValues
//
// Value-type contracts surfaced by the podcast tool surface. These structs are
// the *result* envelopes that lane-10 tools emit — protocols (the "deps")
// produce them; the dispatcher renders them back to the agent.
//
// Kept in a separate file from the protocol surface (`PodcastAgentToolDeps.swift`)
// purely for file-size hygiene — both files import the same types from here.

// MARK: - Playback / queue

/// Where a `play_episode` call should land its item in the playback queue.
public enum QueuePosition: String, Sendable, Equatable {
    /// Start playing immediately. Existing Up Next items are preserved and
    /// resume once this item finishes.
    case now
    /// Insert at the head of Up Next so this item plays after the current
    /// segment/episode ends. Does not interrupt current playback.
    case next
    /// Append to the end of Up Next.
    case end
}

/// Result returned by `play_episode` (both library and external URL paths).
public struct PlayEpisodeResult: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let queuePosition: QueuePosition
    /// `true` when this call started playback immediately (queuePosition == .now).
    /// `false` for `.next` / `.end` — the item was queued but current playback
    /// (if any) is unchanged.
    public let startedPlaying: Bool
    public let episodeTitle: String?
    public let podcastTitle: String?
    public let durationSeconds: Int?

    public init(
        episodeID: EpisodeID,
        queuePosition: QueuePosition,
        startedPlaying: Bool,
        episodeTitle: String? = nil,
        podcastTitle: String? = nil,
        durationSeconds: Int? = nil
    ) {
        self.episodeID = episodeID
        self.queuePosition = queuePosition
        self.startedPlaying = startedPlaying
        self.episodeTitle = episodeTitle
        self.podcastTitle = podcastTitle
        self.durationSeconds = durationSeconds
    }
}

// MARK: - Identifiers

/// Identifier for an episode. Stringly-typed at the tool boundary so we don't
/// couple lane 10 to lane 2/3's `Episode` model. The orchestrator's adapter
/// translates between this string ID and whatever underlying type wins.
public typealias EpisodeID = String

/// Identifier for a podcast subscription. Same rationale as `EpisodeID`.
public typealias PodcastID = String

// MARK: - Search hits

/// A search hit returned by `search_episodes` or `find_similar_episodes`.
public struct EpisodeHit: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let podcastID: PodcastID
    public let title: String
    public let podcastTitle: String
    public let publishedAt: Date?
    public let durationSeconds: Int?
    public let snippet: String?
    public let score: Double?

    public init(
        episodeID: EpisodeID,
        podcastID: PodcastID,
        title: String,
        podcastTitle: String,
        publishedAt: Date? = nil,
        durationSeconds: Int? = nil,
        snippet: String? = nil,
        score: Double? = nil
    ) {
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.title = title
        self.podcastTitle = podcastTitle
        self.publishedAt = publishedAt
        self.durationSeconds = durationSeconds
        self.snippet = snippet
        self.score = score
    }
}

/// A transcript chunk hit returned by `query_transcripts`.
public struct TranscriptHit: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let startSeconds: Double
    public let endSeconds: Double
    public let speaker: String?
    public let text: String
    public let score: Double?

    public init(
        episodeID: EpisodeID,
        startSeconds: Double,
        endSeconds: Double,
        speaker: String? = nil,
        text: String,
        score: Double? = nil
    ) {
        self.episodeID = episodeID
        self.startSeconds = startSeconds
        self.endSeconds = endSeconds
        self.speaker = speaker
        self.text = text
        self.score = score
    }
}

/// A wiki page hit returned by `query_wiki`.
public struct WikiHit: Sendable, Equatable {
    public let pageID: String
    public let title: String
    public let excerpt: String
    public let score: Double?

    public init(pageID: String, title: String, excerpt: String, score: Double? = nil) {
        self.pageID = pageID
        self.title = title
        self.excerpt = excerpt
        self.score = score
    }
}

// MARK: - Composer / summarizer / external lookup

/// A composed briefing artifact. The agent renders this back to the user as a
/// single hero card; lane 8 owns the actual TTS rendering.
public struct BriefingResult: Sendable, Equatable {
    public let briefingID: String
    public let title: String
    public let estimatedSeconds: Int
    public let episodeIDs: [EpisodeID]
    public let scriptPreview: String?

    public init(
        briefingID: String,
        title: String,
        estimatedSeconds: Int,
        episodeIDs: [EpisodeID],
        scriptPreview: String? = nil
    ) {
        self.briefingID = briefingID
        self.title = title
        self.estimatedSeconds = estimatedSeconds
        self.episodeIDs = episodeIDs
        self.scriptPreview = scriptPreview
    }
}

/// A summary returned by `summarize_episode`.
public struct EpisodeSummary: Sendable, Equatable {

    /// Indicates whether the text was produced by an LLM or is a raw fallback.
    public enum SummarySource: String, Sendable, Equatable {
        /// The summary was generated by an LLM from a transcript or show notes.
        case llm
        /// No transcript and no API key were available; the text is the raw
        /// RSS/publisher description, unprocessed.
        case publisherDescription
        /// No episode was found; the summary string is empty.
        case unavailable
    }

    public let episodeID: EpisodeID
    public let summary: String
    public let bulletPoints: [String]
    public let source: SummarySource

    public init(
        episodeID: EpisodeID,
        summary: String,
        bulletPoints: [String] = [],
        source: SummarySource = .llm
    ) {
        self.episodeID = episodeID
        self.summary = summary
        self.bulletPoints = bulletPoints
        self.source = source
    }
}

/// A Perplexity search result.
public struct PerplexityResult: Sendable, Equatable {
    public struct Source: Sendable, Equatable {
        public let title: String
        public let url: String
        public init(title: String, url: String) {
            self.title = title
            self.url = url
        }
    }
    public let answer: String
    public let sources: [Source]

    public init(answer: String, sources: [Source]) {
        self.answer = answer
        self.sources = sources
    }
}

// MARK: - Mutation results

/// Result returned by episode-state mutation tools.
public struct EpisodeMutationResult: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let podcastID: PodcastID?
    public let episodeTitle: String
    public let podcastTitle: String?
    public let state: String

    public init(
        episodeID: EpisodeID,
        podcastID: PodcastID? = nil,
        episodeTitle: String,
        podcastTitle: String? = nil,
        state: String
    ) {
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.episodeTitle = episodeTitle
        self.podcastTitle = podcastTitle
        self.state = state
    }
}

/// Result returned when transcript ingestion is requested.
public struct TranscriptRequestResult: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let status: String
    public let source: String?
    public let message: String?

    public init(
        episodeID: EpisodeID,
        status: String,
        source: String? = nil,
        message: String? = nil
    ) {
        self.episodeID = episodeID
        self.status = status
        self.source = source
        self.message = message
    }
}

/// Result returned by feed-refresh tools.
public struct FeedRefreshResult: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let episodeCount: Int
    public let newEpisodeCount: Int
    public let refreshedAt: Date?

    public init(
        podcastID: PodcastID,
        title: String,
        episodeCount: Int,
        newEpisodeCount: Int,
        refreshedAt: Date? = nil
    ) {
        self.podcastID = podcastID
        self.title = title
        self.episodeCount = episodeCount
        self.newEpisodeCount = newEpisodeCount
        self.refreshedAt = refreshedAt
    }
}

// MARK: - Peer conversation context

/// Per-inbound-turn context the agent's peer tools need to act on the
/// conversation that triggered this run. Built by the Nostr inbound
/// entrypoint and threaded through `PodcastAgentToolDeps`. Owner-chat
/// callers leave this nil; peer-only tools (`end_conversation`,
/// `send_friend_message`) early-return a clean tool error when it's absent.
public struct PeerConversationContext: Sendable, Equatable {
    /// Hex event id of the NIP-10 conversation root. May equal
    /// `inboundEventID` when this is a fresh root.
    public let rootEventID: String
    /// Hex event id of the latest inbound peer event driving this run.
    public let inboundEventID: String
    /// 32-byte x-only public key of the peer (hex-encoded).
    public let peerPubkeyHex: String
    /// `a` tags (NIP-33 parameterized replaceable refs) carried by the root
    /// event. Forwarded verbatim onto outbound replies so the conversation
    /// continues to reference the same coordinate.
    public let rootATags: [[String]]

    public init(
        rootEventID: String,
        inboundEventID: String,
        peerPubkeyHex: String,
        rootATags: [[String]] = []
    ) {
        self.rootEventID = rootEventID
        self.inboundEventID = inboundEventID
        self.peerPubkeyHex = peerPubkeyHex
        self.rootATags = rootATags
    }
}

// MARK: - Inventory rows

/// One subscription row returned by `list_subscriptions`. Compact on purpose:
/// the agent uses this to pick a `PodcastID` for a follow-up tool call (e.g.
/// `list_episodes(podcastID:)` or `query_wiki(scope:)`); detail pages are
/// rendered by other tools.
public struct SubscriptionSummary: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let author: String?
    public let totalEpisodes: Int
    public let unplayedEpisodes: Int
    public let lastPublishedAt: Date?

    public init(
        podcastID: PodcastID,
        title: String,
        author: String?,
        totalEpisodes: Int,
        unplayedEpisodes: Int,
        lastPublishedAt: Date?
    ) {
        self.podcastID = podcastID
        self.title = title
        self.author = author
        self.totalEpisodes = totalEpisodes
        self.unplayedEpisodes = unplayedEpisodes
        self.lastPublishedAt = lastPublishedAt
    }
}

// MARK: - All-podcasts rows

/// One row returned by `list_podcasts`. Covers every `Podcast` known to the
/// store, regardless of whether the user is currently subscribed — mirroring
/// the All Podcasts UI screen. Distinct from `SubscriptionSummary` (which is
/// strictly the subscribed set).
public struct PodcastInventoryRow: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let author: String?
    /// `true` when there is a `PodcastSubscription` row for this podcast.
    public let subscribed: Bool
    public let totalEpisodes: Int
    public let unplayedEpisodes: Int
    public let lastPublishedAt: Date?

    public init(
        podcastID: PodcastID,
        title: String,
        author: String?,
        subscribed: Bool,
        totalEpisodes: Int,
        unplayedEpisodes: Int,
        lastPublishedAt: Date?
    ) {
        self.podcastID = podcastID
        self.title = title
        self.author = author
        self.subscribed = subscribed
        self.totalEpisodes = totalEpisodes
        self.unplayedEpisodes = unplayedEpisodes
        self.lastPublishedAt = lastPublishedAt
    }
}

/// Result returned by `delete_podcast`.
public struct PodcastDeleteResult: Sendable, Equatable {
    public let podcastID: PodcastID
    /// Title at time of deletion (best-effort; nil when the row was already gone).
    public let title: String?
    /// `true` when there was a `PodcastSubscription` for this podcast that the
    /// delete removed alongside the `Podcast` row + episodes.
    public let wasSubscribed: Bool
    public let episodesDeleted: Int

    public init(
        podcastID: PodcastID,
        title: String?,
        wasSubscribed: Bool,
        episodesDeleted: Int
    ) {
        self.podcastID = podcastID
        self.title = title
        self.wasSubscribed = wasSubscribed
        self.episodesDeleted = episodesDeleted
    }
}

// MARK: - Category rows

/// Compact subscription row nested under `list_categories` results.
public struct CategorySubscriptionSummary: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let author: String?

    public init(podcastID: PodcastID, title: String, author: String?) {
        self.podcastID = podcastID
        self.title = title
        self.author = author
    }
}

/// One LLM-derived category returned by `list_categories`.
public struct PodcastCategorySummary: Sendable, Equatable {
    public let categoryID: String
    public let name: String
    public let slug: String
    public let description: String
    public let colorHex: String?
    public let subscriptionCount: Int
    public let generatedAt: Date
    public let model: String?
    public let subscriptions: [CategorySubscriptionSummary]

    public init(
        categoryID: String,
        name: String,
        slug: String,
        description: String,
        colorHex: String?,
        subscriptionCount: Int,
        generatedAt: Date,
        model: String?,
        subscriptions: [CategorySubscriptionSummary]
    ) {
        self.categoryID = categoryID
        self.name = name
        self.slug = slug
        self.description = description
        self.colorHex = colorHex
        self.subscriptionCount = subscriptionCount
        self.generatedAt = generatedAt
        self.model = model
        self.subscriptions = subscriptions
    }
}

/// Category lookup supplied by `change_podcast_category`. The tool accepts ID,
/// slug, or display name so the agent can use whatever `list_categories`
/// returned in the prior turn.
public struct PodcastCategoryReference: Sendable, Equatable {
    public let id: String?
    public let slug: String?
    public let name: String?

    public init(id: String? = nil, slug: String? = nil, name: String? = nil) {
        self.id = id
        self.slug = slug
        self.name = name
    }

    public var isEmpty: Bool {
        [id, slug, name].allSatisfy { ($0 ?? "").isBlank }
    }
}

/// Result returned after moving a show between generated categories.
public struct PodcastCategoryChangeResult: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let previousCategoryID: String?
    public let previousCategoryName: String?
    public let categoryID: String
    public let categoryName: String
    public let categorySlug: String

    public init(
        podcastID: PodcastID,
        title: String,
        previousCategoryID: String?,
        previousCategoryName: String?,
        categoryID: String,
        categoryName: String,
        categorySlug: String
    ) {
        self.podcastID = podcastID
        self.title = title
        self.previousCategoryID = previousCategoryID
        self.previousCategoryName = previousCategoryName
        self.categoryID = categoryID
        self.categoryName = categoryName
        self.categorySlug = categorySlug
    }
}

/// Result returned after the agent creates a clip on behalf of the user.
public struct ClipResult: Sendable, Equatable {
    public let clipID: String
    public let episodeID: EpisodeID
    public let podcastID: PodcastID?
    public let episodeTitle: String
    public let startSeconds: Double
    public let endSeconds: Double
    public let transcriptText: String
    public let caption: String?

    public init(
        clipID: String,
        episodeID: EpisodeID,
        podcastID: PodcastID? = nil,
        episodeTitle: String,
        startSeconds: Double,
        endSeconds: Double,
        transcriptText: String,
        caption: String? = nil
    ) {
        self.clipID = clipID
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.episodeTitle = episodeTitle
        self.startSeconds = startSeconds
        self.endSeconds = endSeconds
        self.transcriptText = transcriptText
        self.caption = caption
    }
}

// MARK: - TTS / Agent podcast publishing

/// One turn inside a `generate_tts_episode` request. Discriminated by `kind`:
/// - `.speech` — synthesised via TTS, using the specified voice.
/// - `.snippet` — an original-audio excerpt spliced from an existing episode.
public struct TTSTurn: Sendable, Equatable {
    public enum Kind: Sendable, Equatable {
        /// Text to synthesise via ElevenLabs TTS. `voiceID` is an ElevenLabs
        /// voice identifier; leave nil to use the agent's configured default.
        case speech(text: String, voiceID: String?)
        /// An original-audio clip from an existing episode to splice in verbatim.
        case snippet(
            episodeID: EpisodeID,
            startSeconds: Double,
            endSeconds: Double,
            label: String?
        )
    }

    public let kind: Kind

    public init(kind: Kind) {
        self.kind = kind
    }
}

/// Result returned by the `generate_tts_episode` tool after the episode is
/// synthesised, stitched, and published to the agent-generated podcast.
public struct TTSEpisodeResult: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let podcastID: PodcastID
    public let title: String
    public let durationSeconds: TimeInterval?
    public let publishedToLibrary: Bool

    public init(
        episodeID: EpisodeID,
        podcastID: PodcastID,
        title: String,
        durationSeconds: TimeInterval? = nil,
        publishedToLibrary: Bool = true
    ) {
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.title = title
        self.durationSeconds = durationSeconds
        self.publishedToLibrary = publishedToLibrary
    }
}

// MARK: - Inventory rows

/// One episode row returned by `list_episodes` / `list_in_progress` /
/// `list_recent_unplayed`. Distinct from `EpisodeHit` (search/RAG result) —
/// inventory rows carry the user's *state* (played, position) instead of a
/// search score.
public struct EpisodeInventoryRow: Sendable, Equatable {
    public let episodeID: EpisodeID
    public let podcastID: PodcastID
    public let title: String
    public let podcastTitle: String
    public let publishedAt: Date?
    public let durationSeconds: Int?
    public let played: Bool
    /// Seconds into the episode the user has reached. `0` for unplayed
    /// or freshly-marked-played; non-zero for in-progress.
    public let playbackPositionSeconds: Double
    /// Convenience flag: `playbackPositionSeconds > 0 && !played`.
    public let isInProgress: Bool

    public init(
        episodeID: EpisodeID,
        podcastID: PodcastID,
        title: String,
        podcastTitle: String,
        publishedAt: Date?,
        durationSeconds: Int?,
        played: Bool,
        playbackPositionSeconds: Double,
        isInProgress: Bool
    ) {
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.title = title
        self.podcastTitle = podcastTitle
        self.publishedAt = publishedAt
        self.durationSeconds = durationSeconds
        self.played = played
        self.playbackPositionSeconds = playbackPositionSeconds
        self.isInProgress = isInProgress
    }
}

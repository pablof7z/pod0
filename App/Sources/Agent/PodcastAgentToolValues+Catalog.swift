import Foundation

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

// MARK: - Episode inventory rows

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

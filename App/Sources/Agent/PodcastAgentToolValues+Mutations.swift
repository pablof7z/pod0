import Foundation

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

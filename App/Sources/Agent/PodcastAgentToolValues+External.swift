import Foundation

// MARK: - External / directory tool value types
//
// Result envelopes for the four external-podcast tools added in the
// search_podcast_directory / subscribe_podcast / play_episode (external path) /
// download_and_transcribe (external) feature pass.

// MARK: - Directory search

/// Search type for `search_podcast_directory`.
public enum PodcastDirectorySearchType: String, Sendable, Equatable {
    case podcast
    case episode
}

/// One hit returned by `search_podcast_directory`.
public struct PodcastDirectoryHit: Sendable, Equatable {
    /// iTunes Collection ID for the podcast.
    public let collectionID: Int?
    public let podcastTitle: String
    public let author: String?
    public let feedURL: String?
    public let artworkURL: String?

    // Episode-level fields (nil when type == .podcast)
    public let episodeTitle: String?
    public let episodeAudioURL: String?
    public let episodeGUID: String?
    public let episodePublishedAt: Date?
    public let episodeDurationSeconds: Int?
    public let episodeDescription: String?

    public init(
        collectionID: Int? = nil,
        podcastTitle: String,
        author: String? = nil,
        feedURL: String? = nil,
        artworkURL: String? = nil,
        episodeTitle: String? = nil,
        episodeAudioURL: String? = nil,
        episodeGUID: String? = nil,
        episodePublishedAt: Date? = nil,
        episodeDurationSeconds: Int? = nil,
        episodeDescription: String? = nil
    ) {
        self.collectionID = collectionID
        self.podcastTitle = podcastTitle
        self.author = author
        self.feedURL = feedURL
        self.artworkURL = artworkURL
        self.episodeTitle = episodeTitle
        self.episodeAudioURL = episodeAudioURL
        self.episodeGUID = episodeGUID
        self.episodePublishedAt = episodePublishedAt
        self.episodeDurationSeconds = episodeDurationSeconds
        self.episodeDescription = episodeDescription
    }
}

// MARK: - Subscribe result

/// Result returned by `subscribe_podcast`.
public struct PodcastSubscribeResult: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let author: String?
    public let feedURL: String
    public let episodeCount: Int
    /// `true` when the show was already in the user's library (idempotent).
    public let alreadySubscribed: Bool

    public init(
        podcastID: PodcastID,
        title: String,
        author: String? = nil,
        feedURL: String,
        episodeCount: Int,
        alreadySubscribed: Bool = false
    ) {
        self.podcastID = podcastID
        self.title = title
        self.author = author
        self.feedURL = feedURL
        self.episodeCount = episodeCount
        self.alreadySubscribed = alreadySubscribed
    }
}

// MARK: - Ensure result

/// Result returned by `PodcastSubscribeProtocol.ensurePodcast(feedURLString:)`.
///
/// Mirrors `PodcastSubscribeResult` minus `alreadySubscribed` — ensure is
/// idempotent by design and never creates a `PodcastSubscription` row, so the
/// caller can't distinguish "we created this just now" from "this was already
/// known." Used by `list_episodes` (external paths) to capture metadata for a
/// feed without forcing a follow.
public struct PodcastEnsureResult: Sendable, Equatable {
    public let podcastID: PodcastID
    public let title: String
    public let author: String?
    public let feedURL: String
    public let episodeCount: Int

    public init(
        podcastID: PodcastID,
        title: String,
        author: String? = nil,
        feedURL: String,
        episodeCount: Int
    ) {
        self.podcastID = podcastID
        self.title = title
        self.author = author
        self.feedURL = feedURL
        self.episodeCount = episodeCount
    }
}

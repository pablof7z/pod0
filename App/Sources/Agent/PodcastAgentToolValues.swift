import Foundation

// MARK: - PodcastAgentToolValues
//
// Value-type contracts surfaced by the podcast tool surface. These structs are
// the *result* envelopes that lane-10 tools emit — protocols (the "deps")
// produce them; the dispatcher renders them back to the agent.
//
// Kept in a separate file from the protocol surface (`PodcastAgentToolDeps.swift`)
// purely for file-size hygiene — both files import the same types from here.
// Split across three files:
//   • PodcastAgentToolValues.swift          — playback, identifiers, search, composer
//   • PodcastAgentToolValues+Mutations.swift — mutation results, peer context, inventory
//   • PodcastAgentToolValues+Catalog.swift   — categories, TTS, episode inventory rows

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

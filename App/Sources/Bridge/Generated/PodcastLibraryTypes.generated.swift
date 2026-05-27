// PodcastLibraryTypes.generated.swift
// Library types: podcast + episode projections, chapters, transcript, ads.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Narrow projection for a subscribed podcast (one library grid/list cell).
/// Episode rows are embedded so the show-detail view doesn't need a second pull.
struct PodcastSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var episodeCount: Int = 0
    var unplayedCount: Int = 0
    var artworkUrl: String? = nil
    var feedUrl: String? = nil
    var author: String? = nil
    /// Podcast description, HTML-stripped by the Rust projection layer.
    /// `nil` when the RSS feed provides no description.
    var description: String? = nil
    /// Per-podcast auto-download policy state. `true` ⇒ the Rust kernel
    /// will auto-queue freshly-discovered episodes on the next feed
    /// refresh. The ShowDetailView toolbar reads this for the toggle's
    /// rendered state and dispatches `set_auto_download` to flip it.
    /// Defaults to `false`; iTunes search rows never set it (they have
    /// no real `PodcastId` server-side).
    var autoDownload: Bool = false
    var episodes: [EpisodeSummary] = []
}

/// One episode row embedded in `PodcastSummary.episodes`.
struct EpisodeSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var podcastId: String? = nil
    var podcastTitle: String? = nil
    var durationSecs: Double? = nil
    var artworkUrl: String? = nil
    /// Unix seconds from `Episode::pub_date`.
    var publishedAt: Int? = nil
    /// On-disk path to the downloaded enclosure when one exists. `nil`
    /// means the episode has not been downloaded yet.
    var downloadPath: String? = nil
    /// Show notes / episode description from the RSS feed, HTML-stripped.
    /// `nil` when empty (D5 — omit to let the host hide the section).
    var description: String? = nil
    /// Publisher-advertised transcript URL (Podcasting 2.0
    /// `<podcast:transcript>` tag).
    var transcriptUrl: String? = nil
    /// Parsed transcript rows. Empty until `podcast.fetch_transcript` succeeds.
    var transcriptEntries: [TranscriptEntry]? = nil
    /// Chapter markers projected after `podcast.fetch_chapters`.
    var chapters: [ChapterSummary]? = nil
    /// Persisted playback position in seconds. `nil` when not started.
    var playbackPositionSecs: Double? = nil
    var transcript: String? = nil
    var aiCategories: [String] = []
    var adSegments: [AdSegment] = []
    var played: Bool = false
    var starred: Bool = false
}

/// One time-stamped transcript row for a single episode.
struct TranscriptEntry: Codable, Equatable, Hashable {
    var startSecs: Double
    var endSecs: Double? = nil
    var speaker: String? = nil
    var text: String
    var aiCategories: [String]? = nil
}

/// Narrow chapter projection for full-player chapter rail rendering.
struct ChapterSummary: Codable, Equatable, Hashable {
    var startSecs: Double
    var endSecs: Double? = nil
    var title: String
    var imageUrl: String? = nil
    var url: String? = nil
    var isAiGenerated: Bool = false
}

/// One advertisement interval inside an episode's audio track.
/// `[startSecs, endSecs)` half-open interval.
struct AdSegment: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var startSecs: Double
    var endSecs: Double
    /// Ad classification: "preroll", "midroll", or "postroll".
    var kind: String = "midroll"
}

/// Snapshot row for a podcast the user owns (NIP-F4 per-podcast keypair).
struct OwnedPodcastInfo: Codable, Identifiable, Equatable, Hashable {
    var podcastId: String
    var podcastPubkeyHex: String
    var showEventJson: String? = nil
    var lastPublishedAt: Int? = nil

    var id: String { podcastId }
}

/// NIP-F4 podcast discovery result row.
struct NostrShowSummary: Codable, Identifiable, Equatable, Hashable {
    var eventId: String
    var authorPubkey: String
    var title: String
    var description: String? = nil
    var feedUrl: String? = nil
    var artworkUrl: String? = nil
    var categories: [String]? = nil

    var id: String { eventId }
}

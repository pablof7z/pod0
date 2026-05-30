// PodcastLibraryTypes.generated.swift
// Library types: podcast + episode projections, chapters, transcript, ads.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Narrow projection for a subscribed podcast (one library grid/list cell).
/// Episode rows are embedded so the show-detail view doesn't need a second pull.
struct PodcastSummary: Identifiable, Equatable, Hashable {
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
    /// `@DefaultFalse`: the Rust projection omits this key when `false` (D5), so
    /// decode must tolerate its absence — synthesized `Decodable` would otherwise
    /// throw `keyNotFound`.
    @DefaultFalse var autoDownload: Bool = false
    /// When `true`, the user explicitly allowed cellular auto-downloads
    /// for this show (Wi-Fi-only is off). Omitted from the wire when `false`
    /// (D5 — `#[serde(skip_serializing_if)]`). The iOS subscription list
    /// uses this to reconstruct `AutoDownloadPolicy.wifiOnly` from the
    /// snapshot rather than hardcoding `wifiOnly: true` for all enabled rows.
    @DefaultFalse var cellularAllowed: Bool = false
    @DefaultEmptyArray var episodes: [EpisodeSummary] = []
}

/// One episode row embedded in `PodcastSummary.episodes`.
struct EpisodeSummary: Identifiable, Equatable, Hashable {
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
    /// Original RSS enclosure URL for streaming. Present for all library
    /// episodes; used by the host player when `downloadPath` is absent.
    var enclosureUrl: String? = nil
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
    // D5 omit-on-empty/false fields — wrapped so absent keys decode to defaults.
    @DefaultEmptyStrings var aiCategories: [String] = []
    @DefaultEmptyArray var adSegments: [AdSegment] = []
    @DefaultFalse var played: Bool = false
    @DefaultFalse var starred: Bool = false
    /// AI Inbox triage decision (`"inbox"` | `"archived"`); `nil` ⇒ untriaged.
    /// Reported by iOS via `set_episode_triage` (M4 / D7), projected back here.
    var triageDecision: String? = nil
    /// `true` when this is the single hero pick of the latest triage pass.
    @DefaultFalse var triageIsHero: Bool = false
    /// One-line "Because …" rationale for `.inbox` picks; `nil` otherwise.
    var triageRationale: String? = nil
    /// `true` once the episode's metadata/transcript chunk is RAG-indexed.
    @DefaultFalse var metadataIndexed: Bool = false
    /// Transient transcript-ingestion status reported by iOS (M4 / D7):
    /// `"queued"` | `"fetching_publisher"` | `"transcribing"` | `"failed"`.
    /// Empty ⇒ no override (idle, or `.ready` derived from `transcript`).
    /// Decoded with a `?? ""` fallback in `init(from:)` since the Rust wire
    /// omits the key when empty (D5).
    var transcriptStatus: String = ""
    /// User-facing error text for `transcriptStatus == "failed"`; `nil` otherwise.
    var transcriptStatusMessage: String? = nil
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
struct ChapterSummary: Equatable, Hashable {
    var startSecs: Double
    var endSecs: Double? = nil
    var title: String
    var imageUrl: String? = nil
    var url: String? = nil
    var isAiGenerated: Bool = false
}

/// One advertisement interval inside an episode's audio track.
/// `[startSecs, endSecs)` half-open interval.
struct AdSegment: Identifiable, Equatable, Hashable {
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

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false) and Vec fields (omit when empty). Swift's synthesized Decodable
// requires every non-optional key to be present, but Rust legitimately omits
// these keys when the value is the zero/default. `decodeIfPresent` + fallback
// makes the decoder forward- and backward-compatible.
//
// WHY extensions, not struct bodies: putting `init(from:)` inside the struct
// body suppresses the synthesized memberwise init. Extensions preserve it.

extension PodcastSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        episodeCount = try c.decodeIfPresent(Int.self, forKey: .episodeCount) ?? 0
        unplayedCount = try c.decodeIfPresent(Int.self, forKey: .unplayedCount) ?? 0
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        feedUrl = try c.decodeIfPresent(String.self, forKey: .feedUrl)
        author = try c.decodeIfPresent(String.self, forKey: .author)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        autoDownload = try c.decodeIfPresent(Bool.self, forKey: .autoDownload) ?? false
        cellularAllowed = try c.decodeIfPresent(Bool.self, forKey: .cellularAllowed) ?? false
        episodes = try c.decodeIfPresent([EpisodeSummary].self, forKey: .episodes) ?? []
    }
}

extension EpisodeSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        podcastId = try c.decodeIfPresent(String.self, forKey: .podcastId)
        podcastTitle = try c.decodeIfPresent(String.self, forKey: .podcastTitle)
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs)
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        publishedAt = try c.decodeIfPresent(Int.self, forKey: .publishedAt)
        downloadPath = try c.decodeIfPresent(String.self, forKey: .downloadPath)
        enclosureUrl = try c.decodeIfPresent(String.self, forKey: .enclosureUrl)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        transcriptUrl = try c.decodeIfPresent(String.self, forKey: .transcriptUrl)
        transcriptEntries = try c.decodeIfPresent([TranscriptEntry].self, forKey: .transcriptEntries)
        chapters = try c.decodeIfPresent([ChapterSummary].self, forKey: .chapters)
        playbackPositionSecs = try c.decodeIfPresent(Double.self, forKey: .playbackPositionSecs)
        transcript = try c.decodeIfPresent(String.self, forKey: .transcript)
        aiCategories = try c.decodeIfPresent([String].self, forKey: .aiCategories) ?? []
        adSegments = try c.decodeIfPresent([AdSegment].self, forKey: .adSegments) ?? []
        played = try c.decodeIfPresent(Bool.self, forKey: .played) ?? false
        starred = try c.decodeIfPresent(Bool.self, forKey: .starred) ?? false
        triageDecision = try c.decodeIfPresent(String.self, forKey: .triageDecision)
        triageIsHero = try c.decodeIfPresent(Bool.self, forKey: .triageIsHero) ?? false
        triageRationale = try c.decodeIfPresent(String.self, forKey: .triageRationale)
        metadataIndexed = try c.decodeIfPresent(Bool.self, forKey: .metadataIndexed) ?? false
        transcriptStatus = try c.decodeIfPresent(String.self, forKey: .transcriptStatus) ?? ""
        transcriptStatusMessage = try c.decodeIfPresent(String.self, forKey: .transcriptStatusMessage)
    }
}

extension AdSegment: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        startSecs = try c.decode(Double.self, forKey: .startSecs)
        endSecs = try c.decode(Double.self, forKey: .endSecs)
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "midroll"
    }
}

extension ChapterSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        startSecs = try c.decode(Double.self, forKey: .startSecs)
        endSecs = try c.decodeIfPresent(Double.self, forKey: .endSecs)
        title = try c.decode(String.self, forKey: .title)
        imageUrl = try c.decodeIfPresent(String.self, forKey: .imageUrl)
        url = try c.decodeIfPresent(String.self, forKey: .url)
        isAiGenerated = try c.decodeIfPresent(Bool.self, forKey: .isAiGenerated) ?? false
    }
}

// PodcastLibraryTypes.generated.swift
// Library types: podcast + episode projections, chapters, transcript, ads.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Narrow projection for a known podcast (one library grid/list cell).
/// Episode rows are embedded so the show-detail view doesn't need a second pull.
struct PodcastSummary: Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var episodeCount: Int = 0
    var unplayedCount: Int = 0
    /// True when the user follows this known podcast. Rust also projects
    /// known-but-unfollowed rows for external feed listing/playback; those
    /// rows must not become `PodcastSubscription`s in Swift.
    var isSubscribed: Bool = true
    var artworkUrl: String? = nil
    var feedUrl: String? = nil
    var author: String? = nil
    /// Podcast description, HTML-stripped by the Rust projection layer.
    /// `nil` when the RSS feed provides no description.
    var description: String? = nil
    /// Unix milliseconds of the last successful Rust feed fetch or 304 check.
    var lastRefreshedAt: Int? = nil
    @DefaultFalse var titleIsPlaceholder: Bool = false
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
    /// Hex public key of the per-podcast NIP-F4 signing key, set once the
    /// podcast has been claimed via `create_owned_podcast`. Drives owned-
    /// podcast UI (the agent's `listOwnedPodcasts` filters on its presence).
    /// `nil` for RSS shows (D5 — omitted on the wire).
    var ownerPubkeyHex: String? = nil
    /// NIP-F4 publish visibility — `"public"` (default) or `"private"`.
    /// The Rust projection omits this when `"public"` (D5).
    var nostrVisibility: String = "public"
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
    /// Size in bytes of the downloaded enclosure, cached by the Rust kernel
    /// at download-completion time. `0` when not downloaded or unknown. Read
    /// directly instead of statting the file on the main actor per tick.
    /// Omitted from the wire when `0` (D5); decoded with a `?? 0` fallback.
    var fileSizeBytes: Int64 = 0
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
    /// AI-generated 2–3 sentence episode summary, projected from the Rust
    /// kernel (`Episode::summary`). `nil` until `podcast.summarize_episode`
    /// runs. Drives the `summarize_episode` agent tool result.
    var summary: String? = nil
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
    /// UUID string of the source episode when this chapter is a clip from
    /// another episode (agent-generated TTS snippet turns). Drives the
    /// clip-source chip + mid-play artwork swap in the player.
    var sourceEpisodeId: String? = nil
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
        isSubscribed = try c.decodeIfPresent(Bool.self, forKey: .isSubscribed) ?? true
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        feedUrl = try c.decodeIfPresent(String.self, forKey: .feedUrl)
        author = try c.decodeIfPresent(String.self, forKey: .author)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        lastRefreshedAt = try c.decodeIfPresent(Int.self, forKey: .lastRefreshedAt)
        titleIsPlaceholder = try c.decodeIfPresent(Bool.self, forKey: .titleIsPlaceholder) ?? false
        autoDownload = try c.decodeIfPresent(Bool.self, forKey: .autoDownload) ?? false
        cellularAllowed = try c.decodeIfPresent(Bool.self, forKey: .cellularAllowed) ?? false
        ownerPubkeyHex = try c.decodeIfPresent(String.self, forKey: .ownerPubkeyHex)
        nostrVisibility = try c.decodeIfPresent(String.self, forKey: .nostrVisibility) ?? "public"
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
        fileSizeBytes = try c.decodeIfPresent(Int64.self, forKey: .fileSizeBytes) ?? 0
        enclosureUrl = try c.decodeIfPresent(String.self, forKey: .enclosureUrl)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        transcriptUrl = try c.decodeIfPresent(String.self, forKey: .transcriptUrl)
        transcriptEntries = try c.decodeIfPresent([TranscriptEntry].self, forKey: .transcriptEntries)
        chapters = try c.decodeIfPresent([ChapterSummary].self, forKey: .chapters)
        playbackPositionSecs = try c.decodeIfPresent(Double.self, forKey: .playbackPositionSecs)
        transcript = try c.decodeIfPresent(String.self, forKey: .transcript)
        summary = try c.decodeIfPresent(String.self, forKey: .summary)
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
        sourceEpisodeId = try c.decodeIfPresent(String.self, forKey: .sourceEpisodeId)
    }
}

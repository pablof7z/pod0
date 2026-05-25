// PodcastTypes.generated.swift
// Generated — historically authored by the codegen pipeline below; that
// pipeline does not exist in the tree yet (no `dump_projection_schemas`
// binary), so this file is hand-maintained from
// `apps/nmp-app-podcast/src/ffi/projections.rs` and
// `apps/nmp-app-podcast/src/ffi/snapshot.rs` until the codegen lands.
// Keep the camelCase shape in sync with the snake_case Rust source — the
// runtime decoder uses `.convertFromSnakeCase` so the rename is implicit.
//
// Intended regeneration command (once the dumper exists):
//
//   cargo run -p nmp-app-podcast --features codegen-schema \
//       --bin dump_projection_schemas \
//     | cargo run -p nmp-codegen -- gen swift
//
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs

import Foundation

/// Top-level snapshot emitted by the Rust podcast kernel on every podcast
/// projection tick (pulled via `nmp_app_podcast_snapshot`).
struct PodcastUpdate: Codable {
    var running: Bool = false
    var rev: Int = 0
    var schemaVersion: Int = 0
    var library: [PodcastSummary] = []
    var nowPlaying: PlayerState? = nil
    var activeAccount: AccountSummary? = nil
    var toast: String? = nil
    var searchResults: [PodcastSummary] = []
    /// NIP-F4 Nostr podcast discovery results.
    var nostrResults: [NostrShowSummary]? = nil
    /// Ordered list of episode ids waiting in the playback queue
    /// ("Up Next"). Mutated kernel-side via `podcast.player.enqueue`,
    /// `dequeue`, `clear_queue`, and `play_next`.
    var queue: [String] = []
    /// App-settings projection. Defaults to the fresh-install state so the
    /// iOS shell can read `snapshot.settings.hasCompletedOnboarding` directly
    /// without an optional-chained `if let`. The Rust side omits the key when
    /// it equals the default, so legacy payloads decode cleanly.
    var settings: SettingsSnapshot = SettingsSnapshot()
    /// Daily briefing projection — `nil` until the scheduler has been
    /// touched at least once.
    var briefing: BriefingSnapshot? = nil
    /// NIP-22 (kind 1111) comments for the currently-playing episode.
    var comments: [CommentSummary] = []
    /// Nostr social graph projection — the active account's NIP-02 (kind:3)
    /// follow list.
    var social: SocialSnapshot? = nil
}

/// App-settings projection emitted alongside `PodcastUpdate`.
///
/// The default value (`hasCompletedOnboarding == false`) is what the wire
/// payload encodes when the Rust kernel skip-serializes an empty settings
/// snapshot — older binaries on `Codable` decode see this as a fresh install.
struct SettingsSnapshot: Codable, Equatable, Hashable {
    var hasCompletedOnboarding: Bool = false
    /// AI-wiki articles surfaced to the per-podcast reader. Mutated
    /// kernel-side via `podcast.wiki.generate` / `delete`. Filtered by
    /// `podcastId` on the iOS side.
    var wikiArticles: [WikiArticle]? = nil
    /// Result of the most recent `podcast.wiki.search`. `nil` until the
    /// first search lands; an empty array means a search with no hits.
    var wikiSearchResults: [WikiArticle]? = nil
}

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
    /// means the episode has not been downloaded yet. Populated by the
    /// Rust `PodcastStore::local_path_for` on each snapshot tick.
    var downloadPath: String? = nil
    /// Show notes / episode description from the RSS feed. `nil` when
    /// the underlying `Episode::description` is empty so the host can
    /// hide the show-notes section without rendering an empty container.
    var description: String? = nil
    /// Plain-text transcript. Populated after a successful
    /// `podcast.fetch_transcript` dispatch; `nil` when not yet fetched
    /// or when no publisher transcript is available for this episode.
    var transcript: String? = nil
    /// Chapter markers projected after a successful `podcast.fetch_chapters`.
    var chapters: [ChapterSummary]? = nil
    /// Persisted playback position in seconds. `nil` when the episode has
    /// not been started (or the user has rewound to 0). Populated by the
    /// Rust `PodcastStore::position_for` on each snapshot tick; drives the
    /// "Resume at X:XX" indicator in the iOS shell.
    var playbackPositionSecs: Double? = nil
}

/// Narrow chapter projection for full-player chapter rail rendering.
///
/// `isAiGenerated == true` for chapters synthesized by
/// `podcast.chapters.compile` (transcript-based stub LLM); `false` for
/// publisher-supplied RSS / Podcasting 2.0 chapters. The iOS shell uses
/// this flag to render a `sparkles` badge in `ChaptersView` so the user
/// can tell at a glance where the boundary came from.
struct ChapterSummary: Codable, Equatable, Hashable {
    var startSecs: Double
    var endSecs: Double? = nil
    var title: String
    var imageUrl: String? = nil
    var url: String? = nil
    var isAiGenerated: Bool = false
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

/// Active player state (present only when an episode is loaded).
struct PlayerState: Codable {
    var episodeId: String? = nil
    var url: String? = nil
    var positionSecs: Double = 0
    var durationSecs: Double? = nil
    var isPlaying: Bool = false
    var isBuffering: Bool = false
    var bufferingFraction: Double = 0
    var speed: Double = 1
    var volume: Double = 1
}

/// Active Nostr identity (present only when an account is loaded).
struct AccountSummary: Codable {
    var npub: String
    var displayName: String? = nil
    var mode: String
    var pictureUrl: String? = nil
}

/// Daily briefing projection — mirrors `BriefingSnapshot` in
/// `apps/nmp-app-podcast/src/ffi/projections.rs`. Present when the
/// scheduler has been touched at least once; the iOS Briefings tab
/// reads it to decide between the empty state (no `briefing` field
/// at all) and a rendered list of segment cards.
struct BriefingSnapshot: Codable, Equatable, Hashable {
    /// One of `"pending"`, `"generating"`, `"ready"`, `"delivered"`,
    /// `"failed"`.
    var status: String = "pending"
    /// `true` while the briefing is being composed. Convenience flag
    /// equivalent to `status == "generating"`.
    var isGenerating: Bool = false
    /// Number of editorial segments in the active briefing.
    var segmentCount: Int = 0
    /// Editorial segments in playback order. Empty until the composer
    /// completes.
    var segments: [BriefingSegmentSummary] = []
    /// Unix seconds the most recent briefing was composed/delivered.
    var lastGeneratedAt: Int? = nil
    /// Minutes until the next scheduled briefing slot on the current
    /// calendar day, when one is configured.
    var nextScheduledMinutes: Int? = nil
}

/// One row in `BriefingSnapshot.segments`.
struct BriefingSegmentSummary: Codable, Equatable, Hashable, Identifiable {
    /// Snake_case label from `podcast_briefings::SegmentKind`:
    /// `"intro"`, `"episode_summary"`, `"new_episode_alert"`,
    /// `"weather_update"`, `"outro_call_to_action"`.
    var kind: String
    /// TTS-narrated body text, plain.
    var text: String
    /// Source podcast title for attribution, when applicable.
    var podcastTitle: String? = nil
    /// Source episode title for attribution, when applicable.
    var episodeTitle: String? = nil

    /// Stable per-render id. Segment order is the projection's order;
    /// combining position + kind + text prefix yields a deterministic
    /// id without needing the composer to mint one.
    var id: String {
        "\(kind)|\(text.prefix(40))"
    }
/// One NIP-22 (kind 1111) comment row in `PodcastUpdate.comments`.
/// Mirrors the Rust-side `CommentSummary` projection. `id` is the
/// Nostr event id (hex). `authorNpub` is the bech32-encoded author
/// pubkey so the iOS shell doesn't need a bech32 dependency to render
/// the truncated stub. `authorName` falls back to `nil` when the
/// projection layer doesn't yet have cached NIP-01 metadata for the
/// author; the UI renders the npub stub in that case. `createdAt` is
/// Unix seconds.
struct CommentSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var authorNpub: String
    var authorName: String? = nil
    var content: String
    var createdAt: Int
/// One contact in the active account's NIP-02 (kind:3) follow list, surfaced
/// via `SocialSnapshot.following` for the iOS "Social" tab.
struct ContactSummary: Codable, Identifiable, Equatable, Hashable {
    /// Bech32 (`npub1…`); doubles as the SwiftUI `Identifiable` id.
    var npub: String
    /// Cached display name from NIP-01 metadata, when known. `nil` means the
    /// grid renders the truncated npub instead.
    var displayName: String? = nil
    /// Cached avatar URL from NIP-01 metadata, when known. `nil` means the
    /// grid renders the initial / fallback avatar.
    var pictureUrl: String? = nil

    var id: String { npub }
}

/// Snapshot of the user's Nostr social graph (NIP-02 / kind:3 follows).
struct SocialSnapshot: Codable, Equatable, Hashable {
    /// Contacts the active account is following. Empty when the contact list
    /// has been fetched but is genuinely empty; the parent `PodcastUpdate.social`
    /// is `nil` (not this struct with an empty `following`) until the
    /// projection layer has populated anything yet.
    var following: [ContactSummary] = []
    /// Number of contacts on the active follow list. Equal to `following.count`
    /// today; surfaced separately so paged variants of `following` keep
    /// working without a second snapshot field.
    var followingCount: Int = 0
/// One row in `PodcastUpdate.wikiArticles` — an AI-synthesised, per-podcast
/// knowledge entry. The scaffold ships with a placeholder `summary`; the
/// LLM-backed follow-up swaps the body in without renegotiating the shape.
struct WikiArticle: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var podcastId: String
    var topic: String
    var summary: String
    var sourceEpisodeIds: [String]? = nil
    /// Unix seconds. Mirrors `WikiArticle::last_updated_at` in Rust.
    var lastUpdatedAt: Int = 0
    var isGenerating: Bool = false
}

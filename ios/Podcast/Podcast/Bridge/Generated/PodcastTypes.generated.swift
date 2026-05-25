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
    /// AI agent picks for the Home rail. Empty until the first
    /// `podcast.picks.refresh` lands (or an implicit refresh fired
    /// at the end of `podcast.refresh_all`).
    var picks: [AgentPickSummary]? = nil
    /// Agent-scheduled tasks projection. Mirrors Rust
    /// `PodcastUpdate.agent_tasks` (see `ffi::projections::AgentTaskSummary`).
    /// Optional so missing-field snapshots decode as `nil` rather than
    /// an empty array.
    var agentTasks: [AgentTaskSummary]? = nil
}

/// One agent-scheduled task surfaced via `PodcastUpdate.agentTasks`.
/// Mirrors Rust `AgentTaskSummary`. Carried as a narrow projection;
/// the iOS view renders directly and mutation flows back through the
/// `podcast.tasks.*` action dispatches.
struct AgentTaskSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var description: String? = nil
    var actionNamespace: String
    var actionBody: String
    var schedule: String
    /// Unix seconds — next scheduled run, when the scheduler has
    /// computed one. `nil` for newly-created tasks.
    var nextRunAt: Int? = nil
    /// Unix seconds — last completed (or failed) run. `nil` until the
    /// task has run at least once.
    var lastRunAt: Int? = nil
    /// One of `"pending"`, `"running"`, `"completed"`, `"failed"`.
    var status: String
    /// `true` when the scheduler should consider this task; toggled
    /// via `enable` / `disable` ops.
    var isEnabled: Bool
    /// RAG / knowledge-base search results, populated after a
    /// `podcast.knowledge.search` action and cleared by
    /// `podcast.knowledge.clear_results`.
    var knowledgeSearchResults: [KnowledgeSearchResult] = []
    /// Agent-memory bag (feature #33). `nil` on the wire (the kernel
    /// omits empty `Vec` payloads); call sites read `memoryFacts ?? []`.
    var memoryFacts: [MemoryFact]? = nil
    /// Agent-generated TTS episodes (feature #43). Empty when the user
    /// hasn't generated any yet — the Rust kernel omits the field in
    /// that case, so the optional decode degrades to `[]`.
    var ttsEpisodes: [TtsEpisodeSummary]? = nil
    /// User-saved audio clips across all episodes (newest-first).
    /// Populated by `podcast.clip.create` / `auto_snip`; emptied by
    /// `podcast.clip.delete`. Absent (`nil`) when no clips exist —
    /// the Rust side skips serializing an empty Vec to preserve the
    /// byte-compatible legacy stub payload.
    var clips: [ClipSummary]? = nil
    /// AI-triaged inbox — unlistened-not-dismissed episodes ranked by a
    /// kernel-side heuristic. Empty when there is nothing to surface.
    var inbox: [InboxItem]? = nil
}

/// One row in the AI-triaged inbox surfaced via `PodcastUpdate.inbox`.
struct InboxItem: Codable, Identifiable, Equatable, Hashable {
    /// `EpisodeId` (hyphenated UUID string) — uniquely identifies the row.
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    /// Unix seconds from `Episode::pub_date`.
    var publishedAt: Int
    var durationSecs: Double? = nil
    /// `0.0..=1.0`; higher = more important.
    var priorityScore: Double
    /// Short caption ("Just published", "Recent", …). `nil` when the
    /// kernel has nothing distinctive to say.
    var priorityReason: String? = nil

    var id: String { episodeId }
    /// NIP-F4 owned podcasts (features #27/#28). Empty until the user
    /// dispatches `podcast.publish.create_owned_podcast` for at least
    /// one podcast.
    var ownedPodcasts: [OwnedPodcastInfo] = []
}

/// Snapshot row for a podcast the user owns (has generated a NIP-F4
/// per-podcast keypair for). `showEventJson` is the most recently
/// constructed unsigned `kind:10154` event JSON (debug surface); the
/// relay-publish path is `relay_pending` until the broader Nostr
/// publishing infrastructure is wired through.
struct OwnedPodcastInfo: Codable, Identifiable, Equatable, Hashable {
    var podcastId: String
    var podcastPubkeyHex: String
    var showEventJson: String? = nil
    /// Unix seconds — when the most recent `publish_show` ran for this podcast.
    var lastPublishedAt: Int? = nil

    /// `Identifiable` conformance — the podcast id is the natural row key.
    var id: String { podcastId }
    /// Voice-mode projection — `nil` while no voice session is active.
    /// Mirrors `crate::ffi::projections::VoiceState`.
    var voice: VoiceSnapshot? = nil
}

/// Voice-mode projection mirroring Rust `VoiceState`. Surfaces both
/// listening (STT) and speaking (TTS) status, the streaming partial
/// transcript while listening, and the most recent assistant reply or
/// committed user utterance under the orb.
struct VoiceSnapshot: Codable, Equatable {
    var isSpeaking: Bool = false
    var isListening: Bool = false
    var currentRequestId: String? = nil
    var currentVoiceId: String? = nil
    var partialTranscript: String? = nil
    var lastResponse: String? = nil
    /// Agent-chat transcript + busy flag. `nil` until the user has sent
    /// their first message during the kernel lifetime; stays non-nil
    /// (with `messages == []`) after a `podcast.agent.clear` so the UI
    /// can distinguish "cleared" from "never opened".
    var agent: AgentSnapshot? = nil
}

/// Agent-chat conversation surfaced via `PodcastUpdate.agent`.
struct AgentSnapshot: Codable, Equatable {
    var messages: [AgentMessageSummary] = []
    /// `true` while the kernel is composing an assistant reply. UI uses
    /// this to disable the send button and render the typing indicator.
    var isBusy: Bool = false
}

/// One row in `AgentSnapshot.messages`.
struct AgentMessageSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    /// `"user"` or `"assistant"`.
    var role: String
    var content: String
    /// Unix seconds (epoch).
    var createdAt: Int
    /// `true` while the assistant is still composing this message
    /// (placeholder bubble with typing indicator).
    var isGenerating: Bool = false
    /// Browse-by-topic aggregate built by the Rust categorizer. Empty
    /// until the first auto-trigger lands (end of every successful feed
    /// refresh) or the iOS shell dispatches `podcast.categorize.run`.
    var categories: [CategoryBrowseItem]? = nil
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
    /// Publisher-advertised transcript URL (Podcasting 2.0
    /// `<podcast:transcript>` tag). When non-nil and `transcriptEntries`
    /// is empty, the viewer renders a "Load Transcript" CTA.
    var transcriptUrl: String? = nil
    /// Parsed transcript rows (speaker / start / end / text). Populated by
    /// the Rust `podcast.fetch_transcript` action after a successful
    /// publisher fetch + parse. Empty until then.
    var transcriptEntries: [TranscriptEntry]? = nil
    /// Chapter markers projected after a successful `podcast.fetch_chapters`.
    var chapters: [ChapterSummary]? = nil
    /// Persisted playback position in seconds. `nil` when the episode has
    /// not been started (or the user has rewound to 0). Populated by the
    /// Rust `PodcastStore::position_for` on each snapshot tick; drives the
    /// "Resume at X:XX" indicator in the iOS shell.
    var playbackPositionSecs: Double? = nil
}

/// One time-stamped transcript row surfaced by the kernel for a single
/// episode. `endSecs` is optional because some sources (publisher plain-text
/// fallbacks, future ingestors) don't emit an end timestamp; the viewer
/// falls back to "largest `startSecs <= position`" in that case.
struct TranscriptEntry: Codable, Equatable, Hashable {
    var startSecs: Double
    var endSecs: Double? = nil
    var speaker: String? = nil
    var text: String
    /// Heuristic topic labels assigned by the Rust categorizer
    /// (`podcast.categorize.run`). Empty until the first run lands;
    /// at most three entries, strongest match first.
    var aiCategories: [String]? = nil
}

/// One row in `PodcastUpdate.categories`. Backs the iOS "Browse by Topic"
/// grid card with category name, episode + podcast counts, and up to
/// three episode ids for the artwork preview stack.
struct CategoryBrowseItem: Codable, Identifiable, Equatable, Hashable {
    var category: String
    var episodeCount: Int = 0
    var podcastCount: Int = 0
    var topEpisodeIds: [String] = []

    var id: String { category }
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
/// One AI agent pick row surfaced via `PodcastUpdate.picks`. Built by the
/// Rust `picks_handler` from a heuristic walk over the library (newest
/// episodes across all shows, capped per show, top-10).
///
/// The podcast title + artwork are denormalized so the Home rail can
/// render a card without doing a second lookup against the library
/// snapshot.
struct AgentPickSummary: Codable, Identifiable, Equatable, Hashable {
    /// Stable id of the underlying episode. Matches `EpisodeSummary.id`.
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    /// Unix seconds.
    var publishedAt: Int = 0
    var durationSecs: Double? = nil
    /// Short reason rendered in the pick chip ("New from {show}").
    var pickReason: String = ""
    /// `0.0..=1.0` — higher is better; used for sort order.
    var pickScore: Double = 0

    /// `Identifiable` conformance uses the episode id, which is stable
    /// across refreshes — the SwiftUI `ForEach` keeps row identity even
    /// when the rank shuffles.
    var id: String { episodeId }
/// One row in the RAG / vector-search projection. The Rust
/// `podcast.knowledge.search` action populates an array of these on the
/// snapshot; the iOS shell renders them in `KnowledgeSearchView`.
///
/// `startSecs` is present when the matched chunk has a timestamp (real
/// transcript chunks will; the current title/description-only stub
/// leaves it `nil`). When present, the row offers a "seek to" button
/// that dispatches `podcast.player.play` + `podcast.player.seek`.
struct KnowledgeSearchResult: Codable, Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastTitle: String
    var snippet: String
    var startSecs: Double? = nil
    /// `0.0...1.0` — drives the relevance bar in the row.
    var relevanceScore: Double = 0

    /// Synthesize a stable identity for `Identifiable` (the wire shape
    /// has no `id` of its own; multiple results can share an episode id
    /// when M6.B starts returning chunk-level hits, so we mix in the
    /// snippet hash to keep `ForEach` happy).
    var id: String { "\(episodeId)|\(snippet.hashValue)" }
/// One row in `PodcastUpdate.memoryFacts` — a single key→value fact the
/// agent or the user wrote so the assistant remembers it across sessions
/// (feature #33). `source` is `"user"` or `"agent"`. `createdAt` is Unix
/// seconds (preserved across upserts).
struct MemoryFact: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var key: String
    var value: String
    var source: String
    var createdAt: Int
/// One agent-generated TTS episode row surfaced via
/// `PodcastUpdate.ttsEpisodes` (feature #43). The `script` is the
/// plain-text body the voice executor will speak when the user taps
/// play. `status` is one of `"generating_script"` | `"ready"` |
/// `"played"`; the iOS list renders it as a chip beside the title.
struct TtsEpisodeSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var script: String
    var durationEstimateSecs: Double
    var createdAt: Int
    var status: String
    var voiceId: String? = nil
/// User-saved audio clip from an episode. One row per saved clip.
/// `start_secs` / `end_secs` are absolute positions inside the episode.
/// `episode_title` / `podcast_title` are re-joined against the live
/// library on every snapshot tick so a podcast rename is visible
/// immediately. `created_at` is Unix seconds.
struct ClipSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var episodeId: String
    var episodeTitle: String
    var podcastTitle: String
    var startSecs: Double
    var endSecs: Double
    var title: String? = nil
    var createdAt: Int
}

//! Emitter — produces the exact text of each Generated/*.generated.swift file.
//!
//! Each `emit_*` function returns `(filename, content)`.
//! All files are returned by `all_files()`.

pub fn all_files() -> Vec<(&'static str, String)> {
    vec![
        ("PodcastTypes.generated.swift",             emit_podcast_types()),
        ("PodcastAgentContextTypes.generated.swift", emit_agent_context_types()),
        ("PodcastDownloadTypes.generated.swift",     emit_download_types()),
        ("PodcastLibraryTypes.generated.swift",      emit_library_types()),
        ("PodcastMediaTypes.generated.swift",        emit_media_types()),
        ("PodcastSocialTypes.generated.swift",       emit_social_types()),
        ("PodcastUpdate.generated.swift",            emit_podcast_update()),
        ("PodcastSettingsSnapshot.generated.swift",  emit_settings_snapshot()),
        ("PodcastPlatformTypes.generated.swift",     emit_platform_types()),
    ]
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn header(file: &str, note: &str, source: &str) -> String {
    format!(
        "// {file}\n// {note}\n// Source of truth: {source}\n\nimport Foundation\n",
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastTypes.generated.swift — legacy redirect stub
// ─────────────────────────────────────────────────────────────────────────────

fn emit_podcast_types() -> String {
    r#"// PodcastTypes.generated.swift
// This file has been split into four focused files in the same directory:
//
//   PodcastUpdate.generated.swift      — PodcastUpdate, PlayerState,
//                                        AccountSummary, DownloadQueueSnapshot,
//                                        DownloadItemSnapshot
//   PodcastSettingsSnapshot.generated.swift
//                                      — SettingsSnapshot
//   PodcastLibraryTypes.generated.swift — PodcastSummary, EpisodeSummary, ChapterSummary,
//                                        TranscriptEntry, AdSegment, OwnedPodcastInfo,
//                                        NostrShowSummary
//   PodcastMediaTypes.generated.swift  — VoiceSnapshot, AgentSnapshot, AgentMessageSummary,
//                                        AgentTaskSummary, AgentPickSummary,
//                                        TtsEpisodeSummary, ClipSummary
//   PodcastSocialTypes.generated.swift — InboxItem, CommentSummary, ContactSummary,
//                                        SocialSnapshot, CategoryBrowseItem,
//                                        KnowledgeSearchResult, MemoryFact
//
// Intended regeneration command (once the dumper exists):
//
//   cargo run -p nmp-app-podcast --features codegen-schema \
//       --bin dump_projection_schemas \
//     | cargo run -p nmp-codegen -- gen swift
//
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastAgentContextTypes.generated.swift
// Source: ffi/projections/agent_context.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_agent_context_types() -> String {
    let hdr = "// PodcastAgentContextTypes.generated.swift\n\
// Hand-maintained mirror of the Rust agent-context projection types.\n\
// Split out of `PodcastUpdate.generated.swift` to keep that file under the\n\
// 500-line hard limit. Keep camelCase in sync with snake_case Rust source —\n\
// `.convertFromSnakeCase` handles the key mapping.\n\
// Source of truth: apps/nmp-app-podcast/src/ffi/projections/agent_context.rs\n\
\n\
import Foundation\n";

    let mut out = hdr.to_string();
    out += r#"
/// Agent-prompt inventory context. Mirrors
/// `ffi::projections::AgentContextSnapshot`. The kernel performs all
/// selection / ordering / capping; `AgentPrompt` only renders the strings.
struct AgentContextSnapshot: Equatable {
    /// Subscribed-show titles, already sorted + capped by the kernel.
    var subscriptions: [String] = []
    /// Followed-show count *before* the cap (drives the "(N)" header and the
    /// "…and N more" suffix).
    var subscriptionsTotal: Int = 0
    /// In-progress episodes (started, not finished, not archived), newest-first.
    var inProgress: [AgentContextEpisode] = []
    /// Recent unplayed episodes inside the recency window, newest-first.
    var recentUnplayed: [AgentContextEpisode] = []
    /// Recency-window width (days) the kernel applied to `recentUnplayed`.
    var recentWindowDays: Int = 0
}

/// One episode row in `AgentContextSnapshot`. Mirrors
/// `ffi::projections::AgentContextEpisode`. Carries the resolved show title
/// so the renderer needs no second lookup.
struct AgentContextEpisode: Equatable {
    var title: String = ""
    var showTitle: String = ""
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on the
// collection fields (omit when empty). `decodeIfPresent` with explicit
// fallbacks keeps the decoder forward- and backward-compatible.

extension AgentContextSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        subscriptions = try c.decodeIfPresent([String].self, forKey: .subscriptions) ?? []
        subscriptionsTotal = try c.decodeIfPresent(Int.self, forKey: .subscriptionsTotal) ?? 0
        inProgress = try c.decodeIfPresent([AgentContextEpisode].self, forKey: .inProgress) ?? []
        recentUnplayed = try c.decodeIfPresent([AgentContextEpisode].self, forKey: .recentUnplayed) ?? []
        recentWindowDays = try c.decodeIfPresent(Int.self, forKey: .recentWindowDays) ?? 0
    }
}

extension AgentContextEpisode: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        showTitle = try c.decodeIfPresent(String.self, forKey: .showTitle) ?? ""
    }
}
"#;
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastDownloadTypes.generated.swift
// Source: ffi/projections/download.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_download_types() -> String {
    r#"// PodcastDownloadTypes.generated.swift
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Active download-queue projection surfaced via `PodcastUpdate.downloads`.
struct DownloadQueueSnapshot: Equatable {
    var active: [DownloadItemSnapshot] = []
    var queuedCount: Int = 0
    var completedToday: Int = 0
}

/// One row in `DownloadQueueSnapshot.active`.
struct DownloadItemSnapshot: Identifiable, Equatable {
    var episodeId: String
    /// What this row fetches. Omitted on the wire for episodes (the default),
    /// so it must decode-default to `.episode`. Lets the model UI pick out its
    /// own rows and lets the episode overlay skip non-episode rows.
    var kind: DownloadKind = .episode
    var progress: Double = 0
    var state: String
    /// Total file size (bytes) once the server reports `Content-Length`.
    /// `nil` until the first HTTP response arrives.
    var totalBytes: Int64? = nil
    var error: String? = nil

    var id: String { episodeId }
}

extension DownloadQueueSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        active = try c.decodeIfPresent([DownloadItemSnapshot].self, forKey: .active) ?? []
        queuedCount = try c.decodeIfPresent(Int.self, forKey: .queuedCount) ?? 0
        completedToday = try c.decodeIfPresent(Int.self, forKey: .completedToday) ?? 0
    }
}

extension DownloadItemSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        kind = try c.decodeIfPresent(DownloadKind.self, forKey: .kind) ?? .episode
        progress = try c.decodeIfPresent(Double.self, forKey: .progress) ?? 0
        state = try c.decode(String.self, forKey: .state)
        totalBytes = try c.decodeIfPresent(Int64.self, forKey: .totalBytes)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastLibraryTypes.generated.swift
// Source: ffi/projections/library.rs, podcast-core/src/types/ad_segment.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_library_types() -> String {
    r#"// PodcastLibraryTypes.generated.swift
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
    /// Typed auto-download mode string (D7). One of `"all_new"`, `"latest_n"`,
    /// or `"off"`. Empty string when the mode is Off (Rust omits the field via
    /// `skip_serializing_if`). The iOS picker reads this to reconstruct
    /// `AutoDownloadPolicy.Mode` precisely — no more conflating `.allNew` and
    /// `.latestN` into a single bool. Prefer this over `autoDownload` for
    /// mode decisions.
    var autoDownloadMode: String = ""
    /// Episode count for `autoDownloadMode == "latest_n"`. `0` for other modes.
    /// Omitted from the wire when `0` (D5).
    var autoDownloadCount: Int = 0
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
    /// User-curated category labels assigned to this podcast by the user.
    /// Empty until the user assigns labels via `set_podcast_user_categories`.
    /// Rust omits the key when empty (D5 — `skip_serializing_if`); decoded
    /// with a `?? []` fallback so absent keys deserialise to an empty array.
    @DefaultEmptyStrings var userCategories: [String] = []
    /// Per-podcast transcription enabled flag. `true` (the default) means
    /// transcription is allowed for this show. The Rust projection omits this
    /// key when `true` (D5 — `skip_serializing_if = "is_true"`).
    @DefaultTrue var transcriptionEnabled: Bool = true
    /// Per-podcast notification policy. `true` (the default) means new-episode
    /// notifications are allowed for this show when global notifications are
    /// also enabled. Rust owns the policy and omits this key when `true`.
    @DefaultTrue var notificationsEnabled: Bool = true
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
    /// Queue-only start boundary for a bounded playback item.
    var queueStartSecs: Double? = nil
    /// Queue-only end boundary for a bounded playback item.
    var queueEndSecs: Double? = nil
    /// Queue-only Rust-owned slot id.
    var queueSlotId: String? = nil
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
        autoDownloadMode = try c.decodeIfPresent(String.self, forKey: .autoDownloadMode) ?? ""
        autoDownloadCount = try c.decodeIfPresent(Int.self, forKey: .autoDownloadCount) ?? 0
        cellularAllowed = try c.decodeIfPresent(Bool.self, forKey: .cellularAllowed) ?? false
        ownerPubkeyHex = try c.decodeIfPresent(String.self, forKey: .ownerPubkeyHex)
        nostrVisibility = try c.decodeIfPresent(String.self, forKey: .nostrVisibility) ?? "public"
        userCategories = try c.decodeIfPresent([String].self, forKey: .userCategories) ?? []
        transcriptionEnabled = try c.decodeIfPresent(Bool.self, forKey: .transcriptionEnabled) ?? true
        notificationsEnabled = try c.decodeIfPresent(Bool.self, forKey: .notificationsEnabled) ?? true
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
        queueStartSecs = try c.decodeIfPresent(Double.self, forKey: .queueStartSecs)
        queueEndSecs = try c.decodeIfPresent(Double.self, forKey: .queueEndSecs)
        queueSlotId = try c.decodeIfPresent(String.self, forKey: .queueSlotId)
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
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastMediaTypes.generated.swift
// Source: ffi/projections/voice.rs, agent.rs, clips.rs; player/state.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_media_types() -> String {
    r#"// PodcastMediaTypes.generated.swift
// Media types: agent, voice, TTS, clips.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Voice-mode projection mirroring Rust `VoiceState`.
struct VoiceSnapshot: Equatable {
    var isSpeaking: Bool = false
    var isListening: Bool = false
    var currentRequestId: String? = nil
    var currentVoiceId: String? = nil
    var partialTranscript: String? = nil
    var lastResponse: String? = nil
}

/// Agent-chat conversation surfaced via `PodcastUpdate.agent`.
struct AgentSnapshot: Equatable {
    var messages: [AgentMessageSummary] = []
    /// `true` while the kernel is composing an assistant reply.
    var isBusy: Bool = false
}

/// One row in `AgentSnapshot.messages`.
struct AgentMessageSummary: Identifiable, Equatable, Hashable {
    var id: String
    /// `"user"` or `"assistant"`.
    var role: String
    var content: String
    var createdAt: Int
    var isGenerating: Bool = false
}

/// One agent-scheduled task surfaced via `PodcastUpdate.agentTasks`.
struct AgentTaskSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var description: String? = nil
    var intentType: String? = nil
    var intentLabel: String? = nil
    var intentDetail: String? = nil
    var schedule: String
    var nextRunAt: Int? = nil
    var lastRunAt: Int? = nil
    /// One of `"pending"`, `"running"`, `"completed"`, `"failed"`.
    var status: String
    var isEnabled: Bool
}

/// One AI agent pick row surfaced via `PodcastUpdate.picks`.
struct AgentPickSummary: Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    var publishedAt: Int = 0
    var durationSecs: Double? = nil
    var pickReason: String = ""
    var pickScore: Double = 0

    var id: String { episodeId }
}

/// One agent-generated TTS episode row surfaced via `PodcastUpdate.ttsEpisodes`.
struct TtsEpisodeSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var title: String
    var script: String
    var durationEstimateSecs: Double
    var createdAt: Int
    var status: String
    var voiceId: String? = nil
}

/// User-saved audio clip from an episode.
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

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false) and Vec fields (omit when empty). Conformance is declared in extensions
// (not struct bodies) so the synthesized memberwise init is preserved.

extension VoiceSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        isSpeaking = try c.decodeIfPresent(Bool.self, forKey: .isSpeaking) ?? false
        isListening = try c.decodeIfPresent(Bool.self, forKey: .isListening) ?? false
        currentRequestId = try c.decodeIfPresent(String.self, forKey: .currentRequestId)
        currentVoiceId = try c.decodeIfPresent(String.self, forKey: .currentVoiceId)
        partialTranscript = try c.decodeIfPresent(String.self, forKey: .partialTranscript)
        lastResponse = try c.decodeIfPresent(String.self, forKey: .lastResponse)
    }
}

extension AgentSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        messages = try c.decodeIfPresent([AgentMessageSummary].self, forKey: .messages) ?? []
        isBusy = try c.decodeIfPresent(Bool.self, forKey: .isBusy) ?? false
    }
}

extension AgentMessageSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        role = try c.decode(String.self, forKey: .role)
        content = try c.decode(String.self, forKey: .content)
        createdAt = try c.decode(Int.self, forKey: .createdAt)
        isGenerating = try c.decodeIfPresent(Bool.self, forKey: .isGenerating) ?? false
    }
}

extension AgentPickSummary: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastId = try c.decode(String.self, forKey: .podcastId)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        publishedAt = try c.decodeIfPresent(Int.self, forKey: .publishedAt) ?? 0
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs)
        pickReason = try c.decodeIfPresent(String.self, forKey: .pickReason) ?? ""
        pickScore = try c.decodeIfPresent(Double.self, forKey: .pickScore) ?? 0
    }
}
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastSocialTypes.generated.swift
// Source: ffi/projections/social.rs, inbox.rs, knowledge.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_social_types() -> String {
    r#"// PodcastSocialTypes.generated.swift
// Social + discovery types: inbox, comments, contacts, categories, knowledge.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// One row in the AI-triaged inbox surfaced via `PodcastUpdate.inbox`.
struct InboxItem: Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    var publishedAt: Int
    var durationSecs: Double? = nil
    /// `0.0..=1.0`; higher = more important.
    var priorityScore: Double
    var priorityReason: String? = nil
    @DefaultEmptyStrings var aiCategories: [String] = []

    var id: String { episodeId }
}

/// One NIP-22 (kind 1111) comment row in `PodcastUpdate.comments`.
struct CommentSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var authorNpub: String
    var authorName: String? = nil
    var content: String
    var createdAt: Int
}

/// One turn within a `NostrConversationDTO`. `direction` is `"inbound"` or
/// `"outbound"` — a plain string so the wire contract is forward-compatible
/// with new directions without a schema bump.
struct NostrConversationTurnDTO: Codable, Identifiable, Equatable, Hashable {
    var eventId: String
    var direction: String
    var pubkeyHex: String
    var createdAt: Int
    var content: String

    var id: String { eventId }
}

/// A NIP-10-threaded conversation between the active account and one peer,
/// surfaced via the `podcast.social` domain projection. Merges inbound
/// kind:1 notes + outbound auto-responder turns under a common root event id.
/// The canonical conversation projection (the flat per-note list was retired).
struct NostrConversationDTO: Codable, Identifiable, Equatable, Hashable {
    var rootEventId: String
    var counterpartyHex: String
    @DefaultEmptyArray var participants: [String] = []
    @DefaultEmptyArray var turns: [NostrConversationTurnDTO] = []
    var trusted: Bool = false
    var peerBlocked: Bool = false
    var peerApproved: Bool = false
    var firstSeen: Int = 0
    var lastActivity: Int = 0

    var id: String { rootEventId }
}

/// One contact in the active account's NIP-02 (kind:3) follow list.
struct ContactSummary: Codable, Identifiable, Equatable, Hashable {
    var npub: String
    /// Raw lowercase-hex pubkey — used by Android claimProfile; iOS can also use
    /// it for resolved_profiles lookup. Decoded via convertFromSnakeCase
    /// (pubkey_hex → pubkeyHex). Empty string on decode failure (never in practice).
    var pubkeyHex: String = ""
    var displayName: String? = nil
    var pictureUrl: String? = nil

    var id: String { npub }
}

/// Snapshot of the user's Nostr social graph (NIP-02 / kind:3 follows).
struct SocialSnapshot: Equatable, Hashable {
    var following: [ContactSummary] = []
    var followingCount: Int = 0
    /// Explicit peer decisions projected from Rust's ApprovedPeerStore.
    var approvedPubkeys: [String] = []
    var blockedPubkeys: [String] = []
}

/// One row in `PodcastUpdate.categories`. Backs the "Browse by Topic" grid.
struct CategoryBrowseItem: Identifiable, Equatable, Hashable {
    var category: String
    var episodeCount: Int = 0
    var podcastCount: Int = 0
    var topEpisodeIds: [String] = []
    var adSegments: [AdSegment]? = nil

    var id: String { category }
}

/// One row in the RAG / vector-search projection.
struct KnowledgeSearchResult: Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastTitle: String
    var snippet: String
    var startSecs: Double? = nil
    var relevanceScore: Double = 0

    var id: String { "\(episodeId)|\(snippet.hashValue)" }
}

/// One key→value fact the agent or user saved via the memory system.
struct MemoryFact: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var key: String
    var value: String
    var source: String
    var createdAt: Int
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false) and Vec fields (omit when empty). Conformance is declared in extensions
// (not struct bodies) so the synthesized memberwise init is preserved.

extension InboxItem: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastId = try c.decode(String.self, forKey: .podcastId)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        publishedAt = try c.decode(Int.self, forKey: .publishedAt)
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs)
        priorityScore = try c.decode(Double.self, forKey: .priorityScore)
        priorityReason = try c.decodeIfPresent(String.self, forKey: .priorityReason)
        aiCategories = try c.decodeIfPresent([String].self, forKey: .aiCategories) ?? []
    }
}

extension SocialSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        following = try c.decodeIfPresent([ContactSummary].self, forKey: .following) ?? []
        followingCount = try c.decodeIfPresent(Int.self, forKey: .followingCount) ?? 0
        approvedPubkeys = try c.decodeIfPresent([String].self, forKey: .approvedPubkeys) ?? []
        blockedPubkeys = try c.decodeIfPresent([String].self, forKey: .blockedPubkeys) ?? []
    }
}

extension CategoryBrowseItem: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        category = try c.decode(String.self, forKey: .category)
        episodeCount = try c.decodeIfPresent(Int.self, forKey: .episodeCount) ?? 0
        podcastCount = try c.decodeIfPresent(Int.self, forKey: .podcastCount) ?? 0
        topEpisodeIds = try c.decodeIfPresent([String].self, forKey: .topEpisodeIds) ?? []
        adSegments = try c.decodeIfPresent([AdSegment].self, forKey: .adSegments)
    }
}

extension KnowledgeSearchResult: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        snippet = try c.decode(String.self, forKey: .snippet)
        startSecs = try c.decodeIfPresent(Double.self, forKey: .startSecs)
        relevanceScore = try c.decodeIfPresent(Double.self, forKey: .relevanceScore) ?? 0
    }
}
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastUpdate.generated.swift
// Source: ffi/snapshot_update.rs, player/state.rs, ffi/projections/identity.rs
// ─────────────────────────────────────────────────────────────────────────────

fn emit_podcast_update() -> String {
    r#"// PodcastUpdate.generated.swift
// Hand-maintained mirror of the Rust projection types until the codegen
// pipeline (`dump_projection_schemas | gen swift`) lands. Keep camelCase in
// sync with snake_case Rust source — `.convertFromSnakeCase` handles it.
// Source of truth: apps/nmp-app-podcast/src/ffi/snapshot.rs

import Foundation

/// Top-level snapshot emitted by the Rust podcast kernel on every podcast
/// projection tick (pulled via `nmp_app_podcast_snapshot`).
struct PodcastUpdate {
    var running: Bool = false
    var rev: Int = 0
    var schemaVersion: Int = 0
    var nowPlaying: PlayerState? = nil
    var downloads: DownloadQueueSnapshot? = nil
    var agent: AgentSnapshot? = nil
    /// Agent-prompt inventory context: kernel-owned selection/ordering/capping
    /// of the subscribed-show list, in-progress episodes, and recent-unplayed
    /// episodes the `AgentPrompt` builder renders into its system prompt.
    /// `nil` when the library is empty.
    var agentContext: AgentContextSnapshot? = nil
    var voice: VoiceSnapshot? = nil
    var social: SocialSnapshot? = nil
    // D5: the Rust projection omits empty collections / default settings from
    // the wire. Wrap them so absent keys decode to defaults instead of throwing
    // `keyNotFound` (synthesized `Decodable` does not honor the `= []` default).
    @DefaultEmptyArray var library: [PodcastSummary] = []
    var activeAccount: AccountSummary? = nil
    var widget: WidgetSnapshot? = nil
    var toast: String? = nil
    @DefaultEmptyArray var searchResults: [PodcastSummary] = []
    @DefaultEmptyArray var nostrResults: [NostrShowSummary] = []
    @DefaultSettings var settings: SettingsSnapshot = SettingsSnapshot()
    @DefaultEmptyArray var comments: [CommentSummary] = []
    @DefaultEmptyArray var queue: [EpisodeSummary] = []
    @DefaultEmptyArray var picks: [AgentPickSummary] = []
    @DefaultEmptyArray var agentTasks: [AgentTaskSummary] = []
    @DefaultEmptyArray var knowledgeSearchResults: [KnowledgeSearchResult] = []
    @DefaultEmptyArray var memoryFacts: [MemoryFact] = []
    @DefaultEmptyArray var ttsEpisodes: [TtsEpisodeSummary] = []
    @DefaultEmptyArray var clips: [ClipSummary] = []
    @DefaultEmptyArray var inbox: [InboxItem] = []
    /// `true` while a background LLM triage pass is running. D5: omitted when false.
    @DefaultFalse var inboxTriageInProgress: Bool = false
    /// Unix seconds for the most recent completed inbox triage pass.
    var inboxLastTriagedAt: Int? = nil
    @DefaultEmptyArray var ownedPodcasts: [OwnedPodcastInfo] = []
    @DefaultEmptyArray var categories: [CategoryBrowseItem] = []
    /// NIP-10-threaded Nostr conversations (inbound + outbound merged), newest-first
    /// by lastActivity. Subsumes the retired flat `agent_notes` list.
    /// Empty until the first `FetchAgentNotes` or outbound auto-reply.
    @DefaultEmptyArray var nostrConversations: [NostrConversationDTO] = []
    /// User-configured app relays (NMP v0.2.1 `configured_relays`). Each row
    /// carries the relay URL plus its NIP-65 role string. Drives the App
    /// Relays editor. Empty until the kernel seeds defaults at start or the
    /// user adds a relay.
    @DefaultEmptyArray var configuredRelays: [AppRelayRow] = []
    /// In-app feedback events (TENEX project notes): kind:1 messages/replies +
    /// kind:513 metadata, all bearing the project `["a"]` coord. Each row is a
    /// `SignedNostrEvent`-shaped object (`pubkey` is the author, `sig` is empty).
    /// Empty until the first `fetch_feedback` dispatch. `FeedbackStore` rebuilds
    /// threads from this flat list (replacing the deleted `FeedbackRelayClient`
    /// WebSocket fetch). Decoded into `FeedbackEventDTO` — NOT `SignedNostrEvent`
    /// — because the snapshot decoder runs `.convertFromSnakeCase`, which would
    /// rename `created_at` and break `SignedNostrEvent`'s explicit coding key.
    @DefaultEmptyArray var feedbackEvents: [FeedbackEventDTO] = []
    /// Resolved feedback threads (#354): the kernel performs the NIP-10
    /// reduction + newest-wins kind:513 metadata; `FeedbackStore` renders these
    /// directly instead of rebuilding threads from `feedbackEvents`.
    @DefaultEmptyArray var feedbackThreads: [FeedbackThreadDTO] = []
}

/// Snapshot-decode mirror of a raw feedback Nostr event, retained for the
/// `FeedbackStore` loading-state check ("has any feedback event arrived?").
/// Thread reduction now happens kernel-side (#354) — see `feedbackThreads` /
/// `FeedbackThreadDTO`. camelCase `createdAt` survives `.convertFromSnakeCase`.
struct FeedbackEventDTO: Codable, Equatable {
    var id: String = ""
    var pubkey: String = ""
    var createdAt: Int = 0
    var kind: Int = 0
    var tags: [[String]] = []
    var content: String = ""

}

/// Snapshot-decode mirror of the kernel's resolved feedback thread (#354).
/// Mirrors `nmp_feedback::projection::FeedbackThreadDto` (snake_case fields
/// survive the decoder's `.convertFromSnakeCase`). The kernel owns the Nostr
/// reduction; the shell maps this to its view `FeedbackThread`.
struct FeedbackThreadDTO: Codable, Equatable {
    var eventId: String = ""
    var authorPubkey: String = ""
    var category: String = ""
    var content: String = ""
    var createdAt: Int = 0
    var title: String? = nil
    var summary: String? = nil
    var statusLabel: String? = nil
    var replies: [FeedbackReplyDTO] = []
}

/// Snapshot-decode mirror of a resolved feedback reply (#354).
/// Mirrors `nmp_feedback::projection::FeedbackReplyDto`.
struct FeedbackReplyDTO: Codable, Equatable {
    var eventId: String = ""
    var authorPubkey: String = ""
    var content: String = ""
    var createdAt: Int = 0
}

/// One configured app relay: URL plus NIP-65 role string
/// (`read` | `write` | `both` | `indexer`, optionally comma-joined).
/// Mirrors `ffi::snapshot_update::AppRelayRow`.
struct AppRelayRow: Codable, Equatable, Identifiable {
    var url: String = ""
    var role: String = ""
    var id: String { url }
}

// AgentContextSnapshot / AgentContextEpisode live in
// `PodcastAgentContextTypes.generated.swift` to keep this file under the
// 500-line hard limit.

/// Active player state (present only when an episode is loaded).
struct PlayerState {
    var episodeId: String? = nil
    var podcastId: String? = nil
    var url: String? = nil
    var positionSecs: Double = 0
    var durationSecs: Double = 0
    var isPlaying: Bool = false
    var bufferingFraction: Double? = nil
    var speed: Float = 1
    var volume: Float = 1
    var sleepTimerRemainingSecs: Int? = nil
    var sleepTimerEndOfEpisode: Bool = false
    var lastError: String? = nil
    /// Set to `true` when AVPlayer fires `AVPlayerItemDidPlayToEndTime`.
    /// Cleared when the next episode loads. Used by the UI to distinguish
    /// a natural finish from a user-initiated stop.
    var didReachNaturalEnd: Bool = false
    /// Absolute end boundary (seconds) for a bounded agent segment.
    /// Nil for unbounded playback.
    var segmentEndSecs: Double? = nil
    /// Title of the chapter active at the current playhead position.
    var currentChapterTitle: String? = nil
    /// Artwork URL of the active chapter, if the chapter has a per-chapter image.
    var currentChapterArtworkUrl: String? = nil
}

/// Active Nostr identity (present only when an account is loaded).
struct AccountSummary: Codable {
    var npub: String
    /// Lowercase 64-hex pubkey. This is the canonical account id; `npub` is
    /// for display.
    var pubkeyHex: String
    /// Short stable account fingerprint derived by Rust from SHA-256(pubkey bytes).
    var fingerprint: String? = nil
    var displayName: String? = nil
    var mode: String
    var pictureUrl: String? = nil
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false), Vec fields (omit when empty), and `settings` (omit when default).
// Swift's synthesized Decodable requires every non-optional key to be present,
// but these keys are legitimately absent from snapshots where the value is the
// zero/default. Custom `init(from:)` in extensions uses `decodeIfPresent` with
// explicit fallbacks so the decoder is forward- and backward-compatible.
//
// WHY extensions, not struct bodies: putting `init(from:)` inside the struct
// body suppresses the synthesized memberwise init. Extensions preserve it.

extension PodcastUpdate: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        running = try c.decodeIfPresent(Bool.self, forKey: .running) ?? false
        rev = try c.decodeIfPresent(Int.self, forKey: .rev) ?? 0
        schemaVersion = try c.decodeIfPresent(Int.self, forKey: .schemaVersion) ?? 0
        nowPlaying = try c.decodeIfPresent(PlayerState.self, forKey: .nowPlaying)
        downloads = try c.decodeIfPresent(DownloadQueueSnapshot.self, forKey: .downloads)
        agent = try c.decodeIfPresent(AgentSnapshot.self, forKey: .agent)
        agentContext = try c.decodeIfPresent(AgentContextSnapshot.self, forKey: .agentContext)
        voice = try c.decodeIfPresent(VoiceSnapshot.self, forKey: .voice)
        social = try c.decodeIfPresent(SocialSnapshot.self, forKey: .social)
        library = try c.decodeIfPresent([PodcastSummary].self, forKey: .library) ?? []
        activeAccount = try c.decodeIfPresent(AccountSummary.self, forKey: .activeAccount)
        widget = try c.decodeIfPresent(WidgetSnapshot.self, forKey: .widget)
        toast = try c.decodeIfPresent(String.self, forKey: .toast)
        searchResults = try c.decodeIfPresent([PodcastSummary].self, forKey: .searchResults) ?? []
        nostrResults = try c.decodeIfPresent([NostrShowSummary].self, forKey: .nostrResults) ?? []
        settings = try c.decodeIfPresent(SettingsSnapshot.self, forKey: .settings) ?? SettingsSnapshot()
        comments = try c.decodeIfPresent([CommentSummary].self, forKey: .comments) ?? []
        queue = try c.decodeIfPresent([EpisodeSummary].self, forKey: .queue) ?? []
        picks = try c.decodeIfPresent([AgentPickSummary].self, forKey: .picks) ?? []
        agentTasks = try c.decodeIfPresent([AgentTaskSummary].self, forKey: .agentTasks) ?? []
        knowledgeSearchResults = try c.decodeIfPresent([KnowledgeSearchResult].self, forKey: .knowledgeSearchResults) ?? []
        memoryFacts = try c.decodeIfPresent([MemoryFact].self, forKey: .memoryFacts) ?? []
        ttsEpisodes = try c.decodeIfPresent([TtsEpisodeSummary].self, forKey: .ttsEpisodes) ?? []
        clips = try c.decodeIfPresent([ClipSummary].self, forKey: .clips) ?? []
        inbox = try c.decodeIfPresent([InboxItem].self, forKey: .inbox) ?? []
        inboxTriageInProgress = try c.decodeIfPresent(Bool.self, forKey: .inboxTriageInProgress) ?? false
        inboxLastTriagedAt = try c.decodeIfPresent(Int.self, forKey: .inboxLastTriagedAt)
        ownedPodcasts = try c.decodeIfPresent([OwnedPodcastInfo].self, forKey: .ownedPodcasts) ?? []
        categories = try c.decodeIfPresent([CategoryBrowseItem].self, forKey: .categories) ?? []
        nostrConversations = try c.decodeIfPresent([NostrConversationDTO].self, forKey: .nostrConversations) ?? []
        configuredRelays = try c.decodeIfPresent([AppRelayRow].self, forKey: .configuredRelays) ?? []
        feedbackEvents = try c.decodeIfPresent([FeedbackEventDTO].self, forKey: .feedbackEvents) ?? []
        feedbackThreads = try c.decodeIfPresent([FeedbackThreadDTO].self, forKey: .feedbackThreads) ?? []
    }
}

extension PlayerState: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decodeIfPresent(String.self, forKey: .episodeId)
        podcastId = try c.decodeIfPresent(String.self, forKey: .podcastId)
        url = try c.decodeIfPresent(String.self, forKey: .url)
        positionSecs = try c.decodeIfPresent(Double.self, forKey: .positionSecs) ?? 0
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs) ?? 0
        isPlaying = try c.decodeIfPresent(Bool.self, forKey: .isPlaying) ?? false
        bufferingFraction = try c.decodeIfPresent(Double.self, forKey: .bufferingFraction)
        speed = try c.decodeIfPresent(Float.self, forKey: .speed) ?? 1
        volume = try c.decodeIfPresent(Float.self, forKey: .volume) ?? 1
        sleepTimerRemainingSecs = try c.decodeIfPresent(Int.self, forKey: .sleepTimerRemainingSecs)
        sleepTimerEndOfEpisode = try c.decodeIfPresent(Bool.self, forKey: .sleepTimerEndOfEpisode) ?? false
        lastError = try c.decodeIfPresent(String.self, forKey: .lastError)
        didReachNaturalEnd = try c.decodeIfPresent(Bool.self, forKey: .didReachNaturalEnd) ?? false
        segmentEndSecs = try c.decodeIfPresent(Double.self, forKey: .segmentEndSecs)
        currentChapterTitle = try c.decodeIfPresent(String.self, forKey: .currentChapterTitle)
        currentChapterArtworkUrl = try c.decodeIfPresent(String.self, forKey: .currentChapterArtworkUrl)
    }
}
"#.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastSettingsSnapshot.generated.swift
// Source: ffi/projections/settings.rs
//
// SettingsSnapshot is the one type that needs a full, explicit `CodingKeys`
// enum: most keys are auto-camelCase, but ~15 fields override to raw
// snake_case (`ollama_chat_url`, `stt_provider`, `assembly_ai_*`, …) and the
// BYOK credential ID/label fields use uppercase acronyms (`...BYOKKeyID`) whose
// `.convertFromSnakeCase` round-trip lands on a *different* converted key
// (`...ByokKeyId`), so the enum must spell those out too.
//
// The `init(from:)` seeds every field from `self.init()` (the canonical Swift
// default mirror of the kernel's `PodcastStore::new()`) and then overwrites
// only the keys present on the wire. Because every field is decoded with
// `decodeIfPresent` and an absent key keeps its property-initializer default,
// the decoder can NEVER throw `keyNotFound` — exactly the D5 omit-on-default
// contract the kernel relies on.
//
// Faithfulness contract: this emitter must produce the body byte-for-byte
// identical to the previously hand-maintained file so the CI drift gate
// (`git diff --exit-code App/Sources/Bridge/Generated`) stays clean and the
// `.convertFromSnakeCase` decode-parity tests (EffectiveSTTProviderDecodeTests,
// SettingsSnapshotParityTests) keep passing unchanged.
// ─────────────────────────────────────────────────────────────────────────────

fn emit_settings_snapshot() -> String {
    r##"// PodcastSettingsSnapshot.generated.swift
// Generated by `cargo run -p nmp-app-podcast --bin swift-codegen`. DO NOT EDIT.
// Mirror of `ffi::projections::SettingsSnapshot`.
//
// This is the one Generated/ type that carries a full explicit `CodingKeys`
// enum: most keys are auto-camelCase, but ~15 fields override to raw snake_case
// (`ollama_chat_url`, `stt_provider`, `assembly_ai_*`, …) and the BYOK
// credential ID/label fields use uppercase acronyms (`...BYOKKeyID`) whose
// `.convertFromSnakeCase` round-trip would otherwise land on a different
// converted key. The custom `init(from:)` seeds every field from `self.init()`
// (the canonical default mirror of the kernel's `PodcastStore::new()`) and
// overwrites only the keys present on the wire, so an absent key keeps its
// default and the decoder can never throw `keyNotFound`.
// Source of truth: apps/nmp-app-podcast/src/ffi/projections/settings.rs

import Foundation

/// App-settings projection. Mirrors `ffi::projections::SettingsSnapshot`.
struct SettingsSnapshot: Equatable {
    var hasCompletedOnboarding: Bool = false
    var autoSkipAdsEnabled: Bool = true
    var autoPlayNext: Bool = true
    var autoMarkPlayedAtEnd: Bool = true
    var headphoneDoubleTapAction: String = "skipForward"
    var headphoneTripleTapAction: String = "clipNow"
    var skipForwardSecs: Double = 30
    var skipBackwardSecs: Double = 15
    var defaultPlaybackRate: Double = 1.0
    var autoDeleteDownloadsAfterPlayed: Bool = false
    var agentInitialModel: String = "deepseek-v4-flash:cloud"
    var agentInitialModelName: String = "DeepSeek Flash"
    var agentThinkingModel: String = "deepseek-v4-pro:cloud"
    var agentThinkingModelName: String = "DeepSeek Pro"
    var memoryCompilationModel: String = "deepseek-v4-flash:cloud"
    var memoryCompilationModelName: String = "DeepSeek Flash"
    var categorizationModel: String = "deepseek-v4-flash:cloud"
    var categorizationModelName: String = "DeepSeek Flash"
    var chapterCompilationModel: String = "deepseek-v4-flash:cloud"
    var chapterCompilationModelName: String = "DeepSeek Flash"
    var embeddingsModel: String = "openai/text-embedding-3-large"
    var embeddingsModelName: String = "text-embedding-3-large"
    var imageGenerationModel: String = "google/gemini-2.5-flash-image"
    var imageGenerationModelName: String = "Gemini 2.5 Flash"
    var rerankerEnabled: Bool = false
    var openRouterCredentialSource: String = ""
    var openRouterKeyPresent: Bool = false
    var openRouterBYOKKeyID: String? = nil
    var openRouterBYOKKeyLabel: String? = nil
    var openRouterConnectedAt: Date? = nil
    var ollamaCredentialSource: String = ""
    var ollamaKeyPresent: Bool = false
    var ollamaBYOKKeyID: String? = nil
    var ollamaBYOKKeyLabel: String? = nil
    var ollamaConnectedAt: Date? = nil
    var ollamaChatURL: String = "https://ollama.com/api/chat"
    var elevenLabsCredentialSource: String = ""
    var elevenLabsKeyPresent: Bool = false
    var elevenLabsBYOKKeyID: String? = nil
    var elevenLabsBYOKKeyLabel: String? = nil
    var elevenLabsConnectedAt: Date? = nil
    var assemblyAICredentialSource: String = ""
    var assemblyAIKeyPresent: Bool = false
    var assemblyAIBYOKKeyID: String? = nil
    var assemblyAIBYOKKeyLabel: String? = nil
    var assemblyAIConnectedAt: Date? = nil
    var perplexityCredentialSource: String = ""
    var perplexityKeyPresent: Bool = false
    var perplexityBYOKKeyID: String? = nil
    var perplexityBYOKKeyLabel: String? = nil
    var perplexityConnectedAt: Date? = nil
    var sttProvider: String = "apple_native"
    var effectiveSttProvider: String = "apple_native"
    var effectiveSttProviderRequiresKey: Bool = false
    var openRouterWhisperModel: String = "openai/whisper-1"
    var assemblyAISTTModel: String = "universal-3-pro,universal-2"
    var elevenLabsSTTModel: String = "scribe_v1"
    var elevenLabsTTSModel: String = "eleven_turbo_v2_5"
    var elevenLabsVoiceID: String = ""
    var elevenLabsVoiceName: String = ""
    var blossomServerURL: String = "https://blossom.primal.net"
    var youtubeExtractorURL: String? = nil
    var localModelID: String? = nil
    var autoIngestPublisherTranscripts: Bool = true
    var autoFallbackToScribe: Bool = true
    var notifyOnNewEpisodes: Bool = true
    var nostrEnabled: Bool = false
    var nostrRelayURL: String = ""
    var nostrProfileName: String = ""
    var nostrProfileAbout: String = ""
    var nostrProfilePicture: String = ""
    var nostrPublicKeyHex: String? = nil
}

extension SettingsSnapshot: Codable {
    enum CodingKeys: String, CodingKey {
        case hasCompletedOnboarding
        case autoSkipAdsEnabled
        case autoPlayNext
        case autoMarkPlayedAtEnd
        case headphoneDoubleTapAction
        case headphoneTripleTapAction
        case skipForwardSecs
        case skipBackwardSecs
        case defaultPlaybackRate
        case autoDeleteDownloadsAfterPlayed
        case agentInitialModel
        case agentInitialModelName
        case agentThinkingModel
        case agentThinkingModelName
        case memoryCompilationModel
        case memoryCompilationModelName
        case categorizationModel
        case categorizationModelName
        case chapterCompilationModel
        case chapterCompilationModelName
        case embeddingsModel
        case embeddingsModelName
        case imageGenerationModel
        case imageGenerationModelName
        case rerankerEnabled
        case openRouterCredentialSource
        case openRouterKeyPresent
        case openRouterBYOKKeyID = "openRouterByokKeyId"
        case openRouterBYOKKeyLabel = "openRouterByokKeyLabel"
        case openRouterConnectedAt
        case ollamaCredentialSource
        case ollamaKeyPresent
        case ollamaBYOKKeyID = "ollamaByokKeyId"
        case ollamaBYOKKeyLabel = "ollamaByokKeyLabel"
        case ollamaConnectedAt
        case ollamaChatURL = "ollama_chat_url"
        case elevenLabsCredentialSource
        case elevenLabsKeyPresent
        case elevenLabsBYOKKeyID = "elevenLabsByokKeyId"
        case elevenLabsBYOKKeyLabel = "elevenLabsByokKeyLabel"
        case elevenLabsConnectedAt
        case assemblyAICredentialSource = "assemblyAiCredentialSource"
        case assemblyAIKeyPresent = "assemblyAiKeyPresent"
        case assemblyAIBYOKKeyID = "assemblyAiByokKeyId"
        case assemblyAIBYOKKeyLabel = "assemblyAiByokKeyLabel"
        case assemblyAIConnectedAt = "assemblyAiConnectedAt"
        case perplexityCredentialSource
        case perplexityKeyPresent
        case perplexityBYOKKeyID = "perplexityByokKeyId"
        case perplexityBYOKKeyLabel = "perplexityByokKeyLabel"
        case perplexityConnectedAt
        case sttProvider = "stt_provider"
        case effectiveSttProvider
        case effectiveSttProviderRequiresKey
        case openRouterWhisperModel = "open_router_whisper_model"
        case assemblyAISTTModel = "assembly_ai_stt_model"
        case elevenLabsSTTModel = "eleven_labs_stt_model"
        case elevenLabsTTSModel = "eleven_labs_tts_model"
        case elevenLabsVoiceID = "eleven_labs_voice_id"
        case elevenLabsVoiceName = "eleven_labs_voice_name"
        case blossomServerURL = "blossom_server_url"
        case youtubeExtractorURL = "youtube_extractor_url"
        case localModelID = "local_model_id"
        case autoIngestPublisherTranscripts = "auto_ingest_publisher_transcripts"
        case autoFallbackToScribe = "auto_fallback_to_scribe"
        case notifyOnNewEpisodes = "notify_on_new_episodes"
        case nostrEnabled = "nostr_enabled"
        case nostrRelayURL = "nostr_relay_url"
        case nostrProfileName = "nostr_profile_name"
        case nostrProfileAbout = "nostr_profile_about"
        case nostrProfilePicture = "nostr_profile_picture"
        case nostrPublicKeyHex = "nostr_public_key_hex"
    }

    init(from decoder: Decoder) throws {
        // Start from the property initializers — the single Swift-side default
        // mirror of the kernel's `PodcastStore::new()` — then overwrite only
        // the keys actually present on the wire. No `?? literal` fallbacks: an
        // absent key keeps the canonical default set by `self.init()`.
        self.init()
        let c = try decoder.container(keyedBy: CodingKeys.self)
        if let v = try c.decodeIfPresent(Bool.self, forKey: .hasCompletedOnboarding) { hasCompletedOnboarding = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoSkipAdsEnabled) { autoSkipAdsEnabled = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoPlayNext) { autoPlayNext = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoMarkPlayedAtEnd) { autoMarkPlayedAtEnd = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .headphoneDoubleTapAction) { headphoneDoubleTapAction = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .headphoneTripleTapAction) { headphoneTripleTapAction = v }
        if let v = try c.decodeIfPresent(Double.self, forKey: .skipForwardSecs) { skipForwardSecs = v }
        if let v = try c.decodeIfPresent(Double.self, forKey: .skipBackwardSecs) { skipBackwardSecs = v }
        if let v = try c.decodeIfPresent(Double.self, forKey: .defaultPlaybackRate) { defaultPlaybackRate = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoDeleteDownloadsAfterPlayed) { autoDeleteDownloadsAfterPlayed = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentInitialModel) { agentInitialModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentInitialModelName) { agentInitialModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentThinkingModel) { agentThinkingModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .agentThinkingModelName) { agentThinkingModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModel) { memoryCompilationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .memoryCompilationModelName) { memoryCompilationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .categorizationModel) { categorizationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .categorizationModelName) { categorizationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModel) { chapterCompilationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .chapterCompilationModelName) { chapterCompilationModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .embeddingsModel) { embeddingsModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .embeddingsModelName) { embeddingsModelName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .imageGenerationModel) { imageGenerationModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .imageGenerationModelName) { imageGenerationModelName = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .rerankerEnabled) { rerankerEnabled = v }
        try decodeCredentialMetadata(c, "openRouter")
        if let v = try c.decodeIfPresent(String.self, forKey: .ollamaChatURL) { ollamaChatURL = v }
        try decodeCredentialMetadata(c, "ollama")
        try decodeCredentialMetadata(c, "elevenLabs")
        try decodeCredentialMetadata(c, "assemblyAI")
        try decodeCredentialMetadata(c, "perplexity")
        if let v = try c.decodeIfPresent(String.self, forKey: .sttProvider) { sttProvider = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .effectiveSttProvider) { effectiveSttProvider = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .effectiveSttProviderRequiresKey) { effectiveSttProviderRequiresKey = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .openRouterWhisperModel) { openRouterWhisperModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .assemblyAISTTModel) { assemblyAISTTModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsSTTModel) { elevenLabsSTTModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsTTSModel) { elevenLabsTTSModel = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceID) { elevenLabsVoiceID = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsVoiceName) { elevenLabsVoiceName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .blossomServerURL) { blossomServerURL = v }
        youtubeExtractorURL = try c.decodeIfPresent(String.self, forKey: .youtubeExtractorURL)
        localModelID = try c.decodeIfPresent(String.self, forKey: .localModelID)
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoIngestPublisherTranscripts) { autoIngestPublisherTranscripts = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .autoFallbackToScribe) { autoFallbackToScribe = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .notifyOnNewEpisodes) { notifyOnNewEpisodes = v }
        if let v = try c.decodeIfPresent(Bool.self, forKey: .nostrEnabled) { nostrEnabled = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrRelayURL) { nostrRelayURL = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfileName) { nostrProfileName = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfileAbout) { nostrProfileAbout = v }
        if let v = try c.decodeIfPresent(String.self, forKey: .nostrProfilePicture) { nostrProfilePicture = v }
        nostrPublicKeyHex = try c.decodeIfPresent(String.self, forKey: .nostrPublicKeyHex)
    }

    private mutating func decodeCredentialMetadata(
        _ c: KeyedDecodingContainer<CodingKeys>,
        _ provider: String
    ) throws {
        switch provider {
        case "openRouter":
            if let v = try c.decodeIfPresent(String.self, forKey: .openRouterCredentialSource) { openRouterCredentialSource = v }
            if let v = try c.decodeIfPresent(Bool.self, forKey: .openRouterKeyPresent) { openRouterKeyPresent = v }
            openRouterBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyID)
            openRouterBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .openRouterBYOKKeyLabel)
            openRouterConnectedAt = try decodeDate(c, .openRouterConnectedAt)
        case "ollama":
            if let v = try c.decodeIfPresent(String.self, forKey: .ollamaCredentialSource) { ollamaCredentialSource = v }
            if let v = try c.decodeIfPresent(Bool.self, forKey: .ollamaKeyPresent) { ollamaKeyPresent = v }
            ollamaBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyID)
            ollamaBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .ollamaBYOKKeyLabel)
            ollamaConnectedAt = try decodeDate(c, .ollamaConnectedAt)
        case "elevenLabs":
            if let v = try c.decodeIfPresent(String.self, forKey: .elevenLabsCredentialSource) { elevenLabsCredentialSource = v }
            if let v = try c.decodeIfPresent(Bool.self, forKey: .elevenLabsKeyPresent) { elevenLabsKeyPresent = v }
            elevenLabsBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyID)
            elevenLabsBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .elevenLabsBYOKKeyLabel)
            elevenLabsConnectedAt = try decodeDate(c, .elevenLabsConnectedAt)
        case "assemblyAI":
            if let v = try c.decodeIfPresent(String.self, forKey: .assemblyAICredentialSource) { assemblyAICredentialSource = v }
            if let v = try c.decodeIfPresent(Bool.self, forKey: .assemblyAIKeyPresent) { assemblyAIKeyPresent = v }
            assemblyAIBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyID)
            assemblyAIBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .assemblyAIBYOKKeyLabel)
            assemblyAIConnectedAt = try decodeDate(c, .assemblyAIConnectedAt)
        case "perplexity":
            if let v = try c.decodeIfPresent(String.self, forKey: .perplexityCredentialSource) { perplexityCredentialSource = v }
            if let v = try c.decodeIfPresent(Bool.self, forKey: .perplexityKeyPresent) { perplexityKeyPresent = v }
            perplexityBYOKKeyID = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyID)
            perplexityBYOKKeyLabel = try c.decodeIfPresent(String.self, forKey: .perplexityBYOKKeyLabel)
            perplexityConnectedAt = try decodeDate(c, .perplexityConnectedAt)
        default:
            break
        }
    }

    private func decodeDate(
        _ c: KeyedDecodingContainer<CodingKeys>,
        _ key: CodingKeys
    ) throws -> Date? {
        guard let timestamp = try c.decodeIfPresent(Int.self, forKey: key) else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(timestamp))
    }
}
"##.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodcastPlatformTypes.generated.swift
// Sources of truth:
//   apps/nmp-app-podcast/src/ffi/projections/platform.rs — WidgetSnapshot
//   apps/podcast-core/src/types/handoff.rs               — HandoffState
// ─────────────────────────────────────────────────────────────────────────────

fn emit_platform_types() -> String {
    r#"// PodcastPlatformTypes.generated.swift
// Generated by `cargo run -p nmp-app-podcast --bin swift-codegen`. DO NOT EDIT.
// Sources of truth:
//   apps/nmp-app-podcast/src/ffi/projections/platform.rs — WidgetSnapshot
//   apps/podcast-core/src/types/handoff.rs               — HandoffState
//
// WidgetSnapshot — embedded in PodcastUpdate and WidgetDomainFrame.
//   Decoded via the bridge's `.convertFromSnakeCase` strategy.
//   NO explicit CodingKeys: synthesised camelCase names are exactly what the
//   strategy produces.  Acronym rule: `artwork_url` → `artworkUrl` (NOT
//   `artworkURL`) because `.convertFromSnakeCase` lowercases every component.
//   Any explicit snake_case CodingKeys here would double-convert and throw
//   `keyNotFound` for every key — the freeze hazard documented in PR #366/#371.
//
// HandoffState — constructed in Swift from kernel snapshot fields; NOT decoded
//   from Rust JSON via the bridge decoder.  Carries explicit CodingKeys for the
//   two fields whose Swift names use uppercase-acronym suffixes (`episodeID`,
//   `podcastID`) that `.convertFromSnakeCase` would otherwise map to `Id`.
//   The manifest's `coding_key_override` records this deviation explicitly.
//
// Manifest: apps/nmp-app-podcast/src/bin/swift_codegen/types.rs

import Foundation

/// Swift mirror of `ffi::projections::WidgetSnapshot`.
///
/// Decoded embedded in `PodcastUpdate` (and `WidgetDomainFrame`) via the
/// bridge's `.convertFromSnakeCase` JSON decoder.
///
/// IMPORTANT — NO explicit `CodingKeys`. The bridge decoder rewrites wire keys
/// (`is_playing` → `isPlaying`) before key lookup, so the synthesised camelCase
/// property names are exactly what is required.  Adding explicit snake_case
/// CodingKeys double-converts and makes every key miss (`keyNotFound`) which
/// fails the entire `PodcastUpdate` decode on every push frame — the freeze
/// regression documented in PR #366/#371.
///
/// Acronym casing follows `.convertFromSnakeCase` semantics: `artwork_url` maps
/// to `artworkUrl` (NOT `artworkURL`), `is_playing` maps to `isPlaying`, etc.
struct WidgetSnapshot: Codable, Equatable {
    /// Title of the active episode, when one is loaded.
    var nowPlayingEpisodeTitle: String? = nil
    /// Title of the podcast/show the active episode belongs to.
    var nowPlayingPodcastTitle: String? = nil
    /// Artwork URL (episode-level preferred, falls back to show).
    var nowPlayingArtworkUrl: String? = nil
    /// Active chapter title at the playhead; nil for chapter-less episodes.
    var nowPlayingChapterTitle: String? = nil
    /// `true` while playback is engaged.
    var isPlaying: Bool = false
    /// Pre-computed progress fraction `0.0..=1.0`.
    var positionFraction: Float = 0
    /// Current playhead in seconds.
    var positionSecs: Double = 0
    /// Track duration in seconds; `0` until reported.
    var durationSecs: Double = 0
    /// Unplayed episode count across subscribed shows.
    var unplayedCount: Int = 0
}

/// Swift mirror of `podcast_core::types::HandoffState`.
///
/// Constructed in Swift from kernel snapshot fields and translated into an
/// `NSUserActivity` by `PlatformCapability.donateHandoff(_:)`.  This type is
/// NOT decoded from Rust JSON via the bridge decoder — the kernel does not emit
/// it as a standalone projection field; the iOS capability builds it from the
/// `now_playing` and `handoff` slice of `PodcastUpdate`.
///
/// Carries explicit `CodingKeys` for `episodeID` and `podcastID` because the
/// Swift names use the uppercase-acronym suffix `ID` while `.convertFromSnakeCase`
/// would produce `Id` (lowercase d) from the wire keys `episode_id` /
/// `podcast_id`.  The `activityType` and `positionSecs` fields round-trip
/// correctly via the strategy and do not need overrides.
struct HandoffState: Codable, Equatable {
    /// `io.f7z.podcast.playing` — playback in progress.
    static let activityPlaying = "io.f7z.podcast.playing"
    /// `io.f7z.podcast.browsing` — non-player surface foregrounded.
    static let activityBrowsing = "io.f7z.podcast.browsing"

    /// `io.f7z.podcast.playing` or `io.f7z.podcast.browsing`.
    var activityType: String
    /// Episode identifier; present for the `playing` activity.
    var episodeID: String? = nil
    /// Podcast identifier; present for `browsing` activity.
    var podcastID: String? = nil
    /// Playhead position in seconds; present when activity is `playing`.
    var positionSecs: Double? = nil

    enum CodingKeys: String, CodingKey {
        case activityType = "activity_type"
        case episodeID = "episode_id"
        case podcastID = "podcast_id"
        case positionSecs = "position_secs"
    }

    /// `true` when `activityType` matches one of the known platform
    /// capability activity ids.
    var isKnownActivityType: Bool {
        switch activityType {
        case Self.activityPlaying, Self.activityBrowsing:
            return true
        default:
            return false
        }
    }
}

/// `userInfo` keys the iOS executor populates on a donated `NSUserActivity`.
/// The receiving side reads back via these keys.
///
/// Field names mirror `HandoffState`'s snake_case wire keys because the
/// kernel's wire shape is the contract.
enum HandoffUserInfoKey {
    static let episodeID = "episode_id"
    static let podcastID = "podcast_id"
    static let positionSecs = "position_secs"
}
"#.to_string()
}

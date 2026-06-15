use podcast_core::ChapterSource;
use serde::{Deserialize, Serialize};

use super::{finite_f64_or_zero};

use crate::player::AdSegment;

/// One known podcast row in the library projection surfaced via
/// [`super::snapshot::PodcastUpdate::library`].
///
/// Narrow enough for the grid/list cells the iOS shell renders; episode
/// rows are embedded so the show-detail view doesn't need a second pull.
/// `is_subscribed` distinguishes followed rows from known-but-unfollowed
/// feeds ingested for external listing/playback.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PodcastSummary {
    /// `PodcastId` as a hyphenated UUID string. For iTunes search results this
    /// is the `collectionId` stringified (no UUID — the feed_url is the key).
    pub id: String,
    pub title: String,
    pub episode_count: usize,
    pub unplayed_count: usize,
    /// True when the user actively follows this known podcast. Known-but-
    /// unfollowed rows are still projected so external listing/playback paths
    /// stay Rust-owned without creating Swift-only state.
    pub is_subscribed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// RSS feed URL. Present for library rows and iTunes search results;
    /// used by `AddShowSheet` to subscribe from a search result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_url: Option<String>,
    /// Podcast author / host name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Podcast description, HTML-stripped and whitespace-collapsed.
    /// Omitted when the RSS feed provides no description (`D5`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Unix milliseconds of the last successful feed fetch/304 check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refreshed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub title_is_placeholder: bool,
    /// Per-podcast auto-download policy state. Mirrors
    /// `PodcastStore::is_auto_download_enabled`. The iOS toolbar toggle
    /// reads this to render its check mark; it dispatches
    /// `PodcastAction::SetAutoDownload` to flip the bit. Defaults to
    /// `false` so the field is omitted from the wire payload (and from
    /// iTunes search rows, which never have a real `PodcastId`).
    ///
    /// Kept for Android back-compat (Android ignores the new typed fields for
    /// now and reads this bool). iOS should prefer `auto_download_mode` +
    /// `auto_download_count` (D7).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_download: bool,
    /// Typed auto-download mode string. One of `"off"`, `"all_new"`, or
    /// `"latest_n"`. Omitted from the wire when `"off"` (D5). The iOS
    /// picker reads this to rehydrate the `AutoDownloadPolicy.Mode`
    /// without conflating `.latestN` and `.allNew` into a single bool.
    ///
    /// Wire contract: snake_case value so `.convertFromSnakeCase` on the
    /// key path (`auto_download_mode` → `autoDownloadMode`) is independent
    /// of the value string. iOS reads the value as a raw `String` and
    /// switches on `"all_new"` / `"latest_n"` / `"off"`.
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub auto_download_mode: String,
    /// Episode count for `mode = "latest_n"`. `0` when the mode is not
    /// `latest_n`. Omitted from the wire when `0` (D5).
    #[serde(default, skip_serializing_if = "zero_u32")]
    pub auto_download_count: u32,
    /// When `true`, cellular auto-download is explicitly allowed for this
    /// show (Wi-Fi-only is off). Omitted from the wire when `false` (D5).
    /// The iOS subscription list reads this to correctly rebuild
    /// `AutoDownloadPolicy.wifiOnly` from the projection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cellular_allowed: bool,
    /// Hex public key of the per-podcast NIP-F4 signing key, set once the
    /// podcast has been claimed via `create_owned_podcast`. Drives the
    /// owned-podcast UI surfaces (`listOwnedPodcasts` filters on its
    /// presence). Omitted when `None` (D5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_pubkey_hex: Option<String>,
    /// NIP-F4 publish visibility — `"public"` or `"private"`. Only meaningful
    /// when `owner_pubkey_hex` is set. Omitted when `"public"` (the default)
    /// to keep the wire payload byte-compatible with older snapshots (D5).
    #[serde(default, skip_serializing_if = "str_is_public")]
    pub nostr_visibility: String,
    /// User-curated category labels assigned to this podcast. Empty when the
    /// user has not assigned any. Per D5 omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_categories: Vec<String>,
    /// Per-podcast transcription enabled flag. `true` (the default) means
    /// transcription is allowed; `false` means it was explicitly disabled by the
    /// user. Omitted from the wire when `true` (D5 — `skip_serializing_if`).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub transcription_enabled: bool,
    /// Recent episodes — ordered newest-first by the projection layer.
    pub episodes: Vec<EpisodeSummary>,
}

impl Default for PodcastSummary {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            episode_count: 0,
            unplayed_count: 0,
            is_subscribed: false,
            artwork_url: None,
            feed_url: None,
            author: None,
            description: None,
            last_refreshed_at: None,
            title_is_placeholder: false,
            auto_download: false,
            auto_download_mode: String::new(),
            auto_download_count: 0,
            cellular_allowed: false,
            owner_pubkey_hex: None,
            nostr_visibility: String::new(),
            user_categories: Vec::new(),
            // The semantic default is `true` (transcription on).
            // serde's `#[serde(default = "default_true")]` applies at deserialization;
            // this manual impl covers Rust's `Default::default()` (e.g. struct-update syntax).
            transcription_enabled: true,
            episodes: Vec::new(),
        }
    }
}

/// D5 skip predicate: omit `nostr_visibility` when it is the `"public"` default.
fn str_is_public(s: &str) -> bool {
    s == "public"
}

/// D5 skip predicate: omit a byte-count field when it is the `0` default
/// (episode not downloaded, or size unknown).
fn zero_i64(v: &i64) -> bool {
    *v == 0
}

/// D5 skip predicate: omit `auto_download_count` when it is `0` (not a
/// `latest_n` mode).
fn zero_u32(v: &u32) -> bool {
    *v == 0
}

/// D5 skip predicate: omit `transcription_enabled` when `true` (the default).
fn is_true(v: &bool) -> bool {
    *v
}

/// Default value for `transcription_enabled` — always `true`.
fn default_true() -> bool {
    true
}

/// One episode row embedded in [`PodcastSummary::episodes`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EpisodeSummary {
    /// `EpisodeId` as a hyphenated UUID string.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// Unix seconds from `Episode::pub_date`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,
    /// On-disk path to the downloaded enclosure, when one exists.
    ///
    /// `None` means the episode has not been downloaded (or its download was
    /// deleted). The host renders a download button in this state; once the
    /// path is `Some`, it renders a "downloaded" indicator instead. Populated
    /// by [`super::snapshot::build_snapshot_payload`] from
    /// `PodcastStore::local_path_for`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_path: Option<String>,
    /// Size in bytes of the downloaded enclosure, cached by the kernel at
    /// download-completion time (`PodcastStore::file_size_for`). `0` when the
    /// episode is not downloaded or its size is unknown. The iOS shell reads
    /// this directly instead of running `URL.resourceValues(.fileSizeKey)` on
    /// the main actor for every downloaded episode on every projection tick.
    /// Per D5 omitted from the wire when `0` to stay byte-compatible with
    /// snapshots that predate the field.
    #[serde(default, skip_serializing_if = "zero_i64")]
    pub file_size_bytes: i64,
    /// The original RSS enclosure URL for streaming. Always present for
    /// library episodes; used by the host player when `download_path` is
    /// absent so it can stream without needing a separate Rust round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosure_url: Option<String>,
    /// Episode description / show notes from the RSS feed.
    ///
    /// `None` when the underlying `Episode::description` is empty so the host
    /// can hide the show-notes section without rendering an empty container.
    /// Populated by [`super::snapshot::build_snapshot_payload`] from
    /// `Episode::description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Publisher-provided transcript URL, when the RSS feed advertises one
    /// via the Podcasting 2.0 `<podcast:transcript>` tag. Surfaced so the iOS
    /// shell can render a "Load Transcript" CTA on episodes that have a
    /// source but have not yet been fetched. Per D5 skipped when `None` to
    /// preserve byte-compat with snapshots that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_url: Option<String>,
    /// Parsed transcript entries (speaker + start/end + text) for the episode,
    /// when one has been fetched via `podcast.fetch_transcript`.
    ///
    /// Per D5 we skip serializing an empty Vec so the wire payload stays
    /// byte-compatible with snapshots that predate this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transcript_entries: Vec<TranscriptEntry>,
    /// Narrow chapter rows projected from `podcast_core::Episode::chapters`
    /// after a `podcast.fetch_chapters` action lands. Empty when the episode
    /// has no chapter markers, or when chapters have not been fetched yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<ChapterSummary>,
    /// Persisted playback position in seconds, when the user has started but
    /// not finished the episode.
    ///
    /// Populated by the snapshot projection from `PodcastStore::position_for`,
    /// which returns `None` when the position is `0.0` (fresh episode) — so
    /// the iOS shell can render a "Resume at X:XX" indicator only on episodes
    /// that have an actual resume point. Per D7 the kernel decides what
    /// counts as "started"; the host only renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback_position_secs: Option<f64>,
    /// Raw plain-text transcript, when one has been fetched and stored via
    /// the transcript write path. Used internally by AI features (chapter
    /// generation, summaries). Per D5 omitted when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    /// AI-generated 2–3 sentence episode summary, projected from the persisted
    /// `Episode::summary` field. Populated by `podcast.summarize_episode`
    /// (off-actor Ollama call). `None` until summarization runs. Per D5 omitted
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Topic labels the agent's heuristic categorizer assigned to this
    /// episode. Empty until `podcast.categorize.run` triggers. Per D5
    /// omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_categories: Vec<String>,
    /// Ad-break intervals for this episode. Per D5 omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_segments: Vec<AdSegment>,
    /// Whether the user has listened to this episode to completion.
    /// Omitted from the wire payload when `false` per D5.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub played: bool,
    /// Whether the user has starred (bookmarked) this episode.
    /// Toggled via `podcast.star_episode`. Omitted when `false` per D5.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starred: bool,
    /// AI Inbox triage decision: `"inbox"` | `"archived"`. `None` means the
    /// episode is untriaged. Reported by iOS via `PodcastAction::SetEpisodeTriage`
    /// (M4 / D7). Per D5 omitted when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage_decision: Option<String>,
    /// `true` when this episode is the single hero pick of the most recent
    /// triage pass. Per D5 omitted when `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub triage_is_hero: bool,
    /// One-line "Because …" rationale shown on the Home Inbox card for
    /// `.inbox` picks. `None` for archived / untriaged episodes. Per D5
    /// omitted when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage_rationale: Option<String>,
    /// `true` once the episode's title+description (or transcript) chunk has
    /// been embedded into the RAG index. Reported by iOS via
    /// `PodcastAction::MarkEpisodesMetadataIndexed` (M4 / D7). Per D5 omitted
    /// when `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub metadata_indexed: bool,
    /// Transient transcript-ingestion status reported by iOS (M4 / D7):
    /// `"queued"` | `"fetching_publisher"` | `"transcribing"` | `"failed"`.
    /// `.ready` is derived by the host from `transcript` presence and is never
    /// surfaced here. Empty string means "no override" (idle / cleared). Per
    /// D5 skipped on the wire when empty.
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub transcript_status: String,
    /// User-facing error text accompanying `transcript_status == "failed"`.
    /// `None` for non-failure statuses. Per D5 omitted when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_status_message: Option<String>,
}

/// One time-stamped transcript row surfaced to the iOS shell.
///
/// Narrow projection of `podcast_transcripts::TranscriptEntry`. `end_secs`
/// is `Option<f64>` so ingestors that don't emit an end timestamp can still
/// surface entries without inventing a value.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TranscriptEntry {
    /// Transcript segment start time in seconds.  Non-finite values (NaN,
    /// Inf) are clamped to `0.0` at the wire boundary so the Swift required
    /// `Double` field never receives JSON `null`.
    #[serde(serialize_with = "finite_f64_or_zero")]
    pub start_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
}

/// Aggregate row consumed by the iOS "Browse by Topic" grid surfaced via
/// [`super::snapshot::PodcastUpdate::categories`].
///
/// Built by [`super::snapshot::build_snapshot_payload`] from the
/// kernel-side categorizer cache (`PodcastHandle::categories`) cross-
/// referenced against the library.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CategoryBrowseItem {
    pub category: String,
    pub episode_count: usize,
    pub podcast_count: usize,
    /// Up to three episode ids, newest-first by `pub_date`. The iOS
    /// shell looks each id up in `library[*].episodes` to render the
    /// preview artwork stack on the category card.
    pub top_episode_ids: Vec<String>,
    /// Ad-break intervals annotated by the upstream ingest pipeline.
    /// Per D5 we skip an empty vec on the wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_segments: Vec<AdSegment>,
}

/// Narrow chapter projection for the player rail. Mirrors the relevant
/// fields of `podcast_core::Chapter` for UI rendering.
///
/// `is_ai_generated` distinguishes chapters synthesized by
/// `podcast.chapters.compile` from publisher-supplied RSS chapters.
/// `source` is the finer-grained provenance ("publisher" / "llm" / "stub")
/// so the UI can signal confidence — an `llm` chapter is transcript-grounded,
/// a `stub` chapter is an offline equal-length placeholder. Omitted on the
/// wire when `publisher` (the default) to keep the payload small and
/// backward-compatible with decoders that predate the field.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ChapterSummary {
    /// Chapter start time in seconds.  Non-finite values (NaN, Inf) are
    /// clamped to `0.0` at the wire boundary: if a NaN propagated here (e.g.
    /// from `ai_chapters::stub_chapters` dividing by a NaN duration), the
    /// Swift bridge would receive JSON `null` for a required `Double` and
    /// throw `keyNotFound`, dropping the entire `PodcastUpdate` frame.
    #[serde(serialize_with = "finite_f64_or_zero")]
    pub start_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub is_ai_generated: bool,
    #[serde(default, skip_serializing_if = "ChapterSource::is_publisher")]
    pub source: ChapterSource,
    /// UUID string of the source episode when this chapter is a clip from
    /// another episode (agent-generated TTS episodes with snippet turns).
    /// Drives the clip-source chip + mid-play artwork swap in the iOS player.
    /// Omitted on the wire when absent (the common publisher-chapter case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

/// NIP-F4 podcast discovery result projected into the iOS Add Show sheet.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct NostrShowSummary {
    pub event_id: String,
    pub author_pubkey: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

#[cfg(test)]
mod episode_size_tests {
    use super::EpisodeSummary;

    #[test]
    fn file_size_bytes_survives_round_trip() {
        let summary = EpisodeSummary {
            id: "e1".into(),
            title: "Downloaded".into(),
            download_path: Some("/var/mobile/e1.mp3".into()),
            file_size_bytes: 8_388_608,
            ..Default::default()
        };
        let json = serde_json::to_string(&summary).expect("encode");
        assert!(json.contains(r#""file_size_bytes":8388608"#));
        let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, summary);
    }

    #[test]
    fn zero_file_size_omitted_on_wire() {
        // D5: a not-downloaded (or unknown-size) episode omits the field so the
        // wire payload stays byte-compatible with snapshots that predate it.
        let summary = EpisodeSummary {
            id: "e2".into(),
            title: "Streaming".into(),
            file_size_bytes: 0,
            ..Default::default()
        };
        let json = serde_json::to_string(&summary).expect("encode");
        assert!(!json.contains("file_size_bytes"));
    }

    #[test]
    fn absent_file_size_decodes_to_zero() {
        // Snapshots written before this field decode cleanly with the default.
        let json = r#"{"id":"e3","title":"Legacy"}"#;
        let decoded: EpisodeSummary = serde_json::from_str(json).expect("decode");
        assert_eq!(decoded.file_size_bytes, 0);
    }
}

#[cfg(test)]
mod owned_field_tests {
    use super::PodcastSummary;

    #[test]
    fn owned_fields_survive_round_trip() {
        let summary = PodcastSummary {
            id: "p1".into(),
            title: "Owned".into(),
            owner_pubkey_hex: Some("deadbeef".into()),
            nostr_visibility: "private".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&summary).expect("encode");
        assert!(json.contains(r#""owner_pubkey_hex":"deadbeef""#));
        assert!(json.contains(r#""nostr_visibility":"private""#));
        let decoded: PodcastSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, summary);
    }

    #[test]
    fn defaults_omit_owned_fields_on_wire() {
        // D5: a plain row omits owner/visibility so the wire payload stays
        // byte-compatible with snapshots that predate these fields.
        let summary = PodcastSummary {
            id: "p2".into(),
            title: "Feed".into(),
            owner_pubkey_hex: None,
            nostr_visibility: "public".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&summary).expect("encode");
        assert!(!json.contains("owner_pubkey_hex"));
        assert!(!json.contains("nostr_visibility"));
    }
}

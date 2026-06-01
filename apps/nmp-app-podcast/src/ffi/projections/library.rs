use podcast_core::ChapterSource;
use serde::{Deserialize, Serialize};

use crate::player::AdSegment;

/// One row in the library projection surfaced via
/// [`super::snapshot::PodcastUpdate::library`].
///
/// Narrow enough for the grid/list cells the iOS shell renders; episode
/// rows are embedded so the show-detail view doesn't need a second pull.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PodcastSummary {
    /// `PodcastId` as a hyphenated UUID string. For iTunes search results this
    /// is the `collectionId` stringified (no UUID — the feed_url is the key).
    pub id: String,
    pub title: String,
    pub episode_count: usize,
    pub unplayed_count: usize,
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
    /// Per-podcast auto-download policy state. Mirrors
    /// `PodcastStore::is_auto_download_enabled`. The iOS toolbar toggle
    /// reads this to render its check mark; it dispatches
    /// `PodcastAction::SetAutoDownload` to flip the bit. Defaults to
    /// `false` so the field is omitted from the wire payload (and from
    /// iTunes search rows, which never have a real `PodcastId`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_download: bool,
    /// When `true`, cellular auto-download is explicitly allowed for this
    /// show (Wi-Fi-only is off). Omitted from the wire when `false` (D5).
    /// The iOS subscription list reads this to correctly rebuild
    /// `AutoDownloadPolicy.wifiOnly` from the projection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cellular_allowed: bool,
    /// Recent episodes — ordered newest-first by the projection layer.
    pub episodes: Vec<EpisodeSummary>,
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

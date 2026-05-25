//! Snapshot projection types — narrow, Codable-friendly mirrors of the
//! Rust-side state machines surfaced via [`super::snapshot::PodcastUpdate`].
//!
//! Lives in its own module to keep [`super::snapshot`] focused on the
//! C-ABI entry points and the typed root struct. Each projection here
//! is the *external* (FFI-wire) shape; the *internal* state machines
//! it derives from live in their domain crates (`podcast-briefings`,
//! `podcast-agent-core`, …) or in this crate's domain modules
//! (`crate::player`, `crate::download`).
//!
//! ## D7 / D6
//!
//! These structs are pure data: Swift `Codable` decodes them and renders.
//! No conditional logic, no policy decisions — the projection layer
//! that *builds* them owns those, and is colocated with the kernel-side
//! action modules in subsequent milestones (M3.B / M4.B / M7.B / M8.B
//! / M9.B).

use serde::{Deserialize, Serialize};

/// Snapshot of the [`crate::download::DownloadQueue`] surfaced to the iOS
/// shell via `PodcastUpdate.downloads`.
///
/// Designed so the UI can render the Downloads section (Settings →
/// Downloads, EpisodeRow capsule) directly from this payload without
/// reaching back into Rust:
///
/// * `active` — every item that holds a slot (Active or Paused) plus
///   any item still in `Queued` state, with progress + state surfaced.
/// * `queued_count` — number of items in `Queued` state (subset of
///   `active.len()` with `state == "queued"`); provided as a sugar so
///   the UI doesn't need to filter.
/// * `completed_today` — the number of items that completed in the
///   current wall-clock day. Computed by the projection layer that
///   builds this snapshot (it has access to the wall clock that the
///   queue itself doesn't); the queue itself doesn't track timestamps
///   in M4.A. M4.B will refine this once auto-download policy lands.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DownloadQueueSnapshot {
    /// Items currently visible to the user (Active, Paused, Queued, or
    /// most-recent Failed). The ordering is the projection's choice —
    /// the queue itself uses a FIFO `queue_order`, but the snapshot
    /// builder can re-order for UI grouping.
    pub active: Vec<DownloadItemSnapshot>,
    /// Number of items still in `Queued` state.
    pub queued_count: usize,
    /// Number of items that transitioned to `Completed` today
    /// (wall-clock). Zero in M4.A — wired in M4.B where the policy
    /// layer has a clock.
    pub completed_today: usize,
}

/// One row in [`DownloadQueueSnapshot::active`].
///
/// `state` is a string (`"active"` / `"queued"` / `"paused"` /
/// `"failed"`) rather than the [`crate::download::DownloadItemState`]
/// enum because the snapshot is consumed by Swift `Codable` decoders
/// that prefer string discriminators over enum variants when the
/// downstream view model only switches on a handful of states.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DownloadItemSnapshot {
    pub episode_id: String,
    /// `0.0..=1.0`, or `0.0` when `total_bytes` is unknown.
    pub progress: f32,
    /// One of `"active"`, `"queued"`, `"paused"`, `"failed"`. Successful
    /// completions and explicit cancellations drop out of `active` (the
    /// projection layer decides whether to retain a brief "just
    /// finished" banner).
    pub state: String,
    /// Most recent failure diagnostic, when `state == "failed"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Snapshot of the agent-chat projection surfaced via
/// [`super::snapshot::PodcastUpdate::agent`].
///
/// Built by the future M7.B action-module wiring from a
/// [`podcast_agent_core::ConversationActor`]. Kept narrow on purpose:
/// the UI needs the active count + the pending-approvals queue + the
/// id of the most recently touched conversation; the rest of the
/// conversation rows are paged in by separate accessors.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ConversationsSnapshot {
    /// Number of conversations the actor is currently tracking.
    pub active_count: usize,
    /// Outstanding approvals the user still has to decide on.
    ///
    /// Sorted oldest-first by the projection layer
    /// (`podcast_agent_core::sorted_active_approvals`) so the UI can
    /// render the queue without re-sorting.
    pub pending_approvals: Vec<PendingApprovalSnapshot>,
    /// Most recently touched conversation id (UUID rendered as the
    /// canonical hyphenated string), or `None` when the actor is
    /// empty. Surfaced as `String` rather than typed `Uuid` so the
    /// iOS shell's Codable decoder can treat it as a plain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_conversation_id: Option<String>,
}

/// One row in [`ConversationsSnapshot::pending_approvals`].
///
/// `requested_at` is surfaced as a Unix timestamp (seconds since
/// epoch) rather than ISO-8601 so the iOS view model can compare
/// against `Date()` without a formatter round-trip — matches the
/// pattern the legacy `NostrPendingApproval` view code already uses.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PendingApprovalSnapshot {
    pub id: String,
    pub description: String,
    /// Unix seconds — see struct-level comment.
    pub requested_at: i64,
}

/// Snapshot of the voice (TTS) session surfaced via
/// [`super::snapshot::PodcastUpdate::voice`].
///
/// Mirrors `crate::capability::voice::VoiceCommand` / `VoiceReport`
/// state on the kernel side: `is_speaking` flips to `true` when the
/// executor reports `Started`, back to `false` on `Finished` / `Failed`
/// / `Stopped`. `current_request_id` is the in-flight TTS correlation
/// id (matching the legacy Swift `VoiceTurn` request id);
/// `current_voice_id` is the active voice the user / agent selected.
///
/// `current_request_id` and `current_voice_id` are `Option` because
/// the UI may need to render "speaking but voice id not yet bound"
/// (mid-fallback) or "idle but voice id remembered" (between turns) —
/// surfacing both fields independently saves a re-derivation in Swift.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct VoiceState {
    /// `true` while a `Speak` is in flight (between `Started` and the
    /// terminal `Finished` / `Failed` / `Stopped`).
    pub is_speaking: bool,
    /// `true` while on-device speech recognition is running (between
    /// `ListeningStarted` and `ListeningStopped`). Drives the
    /// pulsing-microphone affordance in `VoiceModeView`.
    #[serde(default)]
    pub is_listening: bool,
    /// Correlation id of the in-flight `Speak`, mirrored from the
    /// `VoiceCommand::Speak.request_id`. `None` when nothing is in
    /// flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_request_id: Option<String>,
    /// The voice id the executor is currently configured to use.
    /// Set by the most recent `SetVoice` or by the explicit
    /// `voice_id` on a `Speak`. `None` until the user picks one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_voice_id: Option<String>,
    /// Streaming best-guess transcript while `is_listening == true`.
    /// Updated on every [`crate::capability::VoiceReport::TranscriptPartial`]
    /// report; cleared back to `None` on `TranscriptFinal` /
    /// `ListeningStopped`. The UI binds the voice-mode caption to this
    /// field so chunked recognition results render with no extra
    /// buffering on the Swift side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_transcript: Option<String>,
    /// Most recent committed transcript or assistant reply the UI
    /// surfaces under the voice-mode orb. Updated by the kernel on
    /// `TranscriptFinal` (the user said this) or on a `Speak` action
    /// (the assistant said this). `None` between sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_response: Option<String>,
}

/// Snapshot of the briefing scheduler surfaced via
/// [`super::snapshot::PodcastUpdate::briefing`].
///
/// Mirrors `podcast_briefings::BriefingScheduler` state with the
/// projections the UI needs:
///
/// * `status` — the lifecycle label of the active briefing
///   (`"pending"` / `"generating"` / `"ready"` / `"delivered"` /
///   `"failed"`). The full enum lives in `podcast-briefings`; here we
///   surface it as a string so the Swift decoder doesn't need the
///   enum variant case-mapping.
/// * `is_generating` — convenience flag (`status == "generating"`)
///   the iOS view binds against to render a spinner without
///   re-deriving from the label.
/// * `segment_count` — number of editorial segments produced (0
///   until `status == "ready"`).
/// * `segments` — the editorial segments themselves, projected to a
///   narrow Codable shape (`kind`, `text`, attribution titles).
///   Empty until the composer completes.
/// * `last_generated_at` — Unix seconds the most recent briefing
///   was composed/delivered. `None` until the first briefing
///   finishes.
/// * `next_scheduled_minutes` — minutes until the next scheduled
///   briefing today, when the scheduler has an active schedule that
///   covers today and the slot hasn't passed yet.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSnapshot {
    /// One of `"pending"`, `"generating"`, `"ready"`, `"delivered"`,
    /// `"failed"` — matches `podcast_briefings::BriefingStatus::label`.
    pub status: String,
    /// `true` while the briefing is being composed. Convenience flag
    /// equivalent to `status == "generating"`; surfaced separately
    /// so the iOS view doesn't reach into the label.
    pub is_generating: bool,
    /// Number of editorial segments in the active briefing. Zero
    /// until the composer completes.
    pub segment_count: usize,
    /// Editorial segments in playback order. Empty until the composer
    /// completes (`status == "ready"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<BriefingSegmentSummary>,
    /// Unix seconds the most recent briefing was composed/delivered.
    /// `None` until the first briefing finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_generated_at: Option<i64>,
    /// Minutes until the next scheduled briefing slot on the current
    /// calendar day. `None` when no schedule is active, when today
    /// isn't covered, or when the slot has already passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_scheduled_minutes: Option<u32>,
}

/// One row in [`BriefingSnapshot::segments`].
///
/// Narrow Codable mirror of `podcast_briefings::BriefingSegment` — the
/// fields the iOS card view actually renders. The richer internal
/// segment shape (`episode_id`, `duration_hint_secs`) stays in
/// `podcast-briefings`; the projection surfaces titles instead of ids
/// so the view doesn't need to look up the library to render
/// attribution.
///
/// `kind` is the snake_case label from `podcast_briefings::SegmentKind`
/// (`"intro"`, `"episode_summary"`, `"new_episode_alert"`,
/// `"weather_update"`, `"outro_call_to_action"`) so the Swift decoder
/// can switch on a plain string.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSegmentSummary {
    /// Editorial category — see struct-level doc.
    pub kind: String,
    /// TTS-narrated body text, plain.
    pub text: String,
    /// Source podcast title for attribution; `None` for intro /
    /// outro / weather.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_title: Option<String>,
    /// Source episode title for attribution; `None` for intro /
    /// outro / weather.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_title: Option<String>,
}

/// Narrow projection consumed by the M11 platform-integration
/// executors (widget extension, Live Activity, Handoff,
/// Siri shortcuts). It is **not** a superset of `now_playing` —
/// the shape is intentionally lossy so the platform extensions
/// don't have to depend on the full player + downloads schemas.
///
/// Per D7 the kernel chooses what to surface; if a field is
/// missing here, the widget renders its empty state. The Rust
/// projection layer builds this from `PlayerState` +
/// `DownloadQueue` + the unplayed-episode count on each tick;
/// the iOS shell serializes it into the App Group `UserDefaults`
/// key the widget extension reads (see
/// `PlatformCapability.writeWidgetSnapshot(_:)`).
///
/// `position_fraction` is pre-computed (`position_secs /
/// duration_secs`, clamped to `0.0..=1.0`) so the widget can
/// render a progress ring without doing math on possibly-zero
/// duration; `0.0` is the safe default both for "no episode"
/// and "duration unknown".
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WidgetSnapshot {
    /// Title of the active episode, when one is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_episode_title: Option<String>,
    /// Title of the podcast/show the active episode belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_podcast_title: Option<String>,
    /// Artwork URL (episode-level preferred, falls back to show).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_artwork_url: Option<String>,
    /// `true` while playback is engaged (the player's `is_playing`).
    pub is_playing: bool,
    /// Pre-computed progress fraction `0.0..=1.0`; the widget renders
    /// this as a ring/bar without re-deriving from secs+duration.
    pub position_fraction: f32,
    /// Number of unplayed episodes across all subscribed shows;
    /// drives the badge / "X to listen" line in the widget.
    pub unplayed_count: usize,
}

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
    /// Per-podcast auto-download policy state. Mirrors
    /// `PodcastStore::is_auto_download_enabled`. The iOS toolbar toggle
    /// reads this to render its check mark; it dispatches
    /// `PodcastAction::SetAutoDownload` to flip the bit. Defaults to
    /// `false` so the field is omitted from the wire payload (and from
    /// iTunes search rows, which never have a real `PodcastId`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_download: bool,
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
    /// source but have not yet been fetched. Populated by
    /// [`super::snapshot::build_snapshot_payload`] from
    /// `Episode::publisher_transcript_url`. Per D5 skipped when `None` to
    /// preserve byte-compat with snapshots that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_url: Option<String>,
    /// Parsed transcript entries (speaker + start/end + text) for the episode,
    /// when one has been fetched via `podcast.fetch_transcript`.
    ///
    /// Populated by the snapshot builder from the per-episode transcript cache
    /// on `PodcastHandle`. Empty when the user has not yet dispatched
    /// `podcast.fetch_transcript`, or when the most recent fetch produced no
    /// usable entries (parse failure, HTTP error). The iOS shell renders the
    /// "Load Transcript" CTA in those cases when `transcript_url` is set.
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
}

/// One time-stamped transcript row surfaced to the iOS shell.
///
/// Narrow projection of `podcast_transcripts::TranscriptEntry`. The full
/// domain type carries optional per-word timestamps for karaoke-style
/// highlighting; the FFI projection drops them because the M14 iOS viewer
/// renders segment-level only. `end_secs` is `Option<f64>` here (the source
/// type is required `f64`) so future ingestors that don't emit an end
/// timestamp can still surface entries without inventing a value — the
/// viewer falls back to "the entry whose `start_secs` is the largest
/// `<= position`" in that case.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TranscriptEntry {
    pub start_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
}

/// One row in the AI-triaged inbox surfaced via
/// [`super::snapshot::PodcastUpdate::inbox`]. The kernel projection is built
/// by [`crate::inbox_handler::build_inbox`] from the unlistened-∖-dismissed
/// set; the score is normalised to `0.0..=1.0` (higher = more important).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InboxItem {
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_id: String,
    pub podcast_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// Unix seconds (`Episode::pub_date.timestamp()`).
    pub published_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// `0.0..=1.0`; higher = more important.
    pub priority_score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_reason: Option<String>,
}

/// Narrow chapter projection for the player rail. Mirrors the relevant
/// fields of `podcast_core::Chapter` for UI rendering.
///
/// `is_ai_generated` distinguishes chapters synthesized by
/// `podcast.chapters.compile` (the LLM-stub path that splits a transcript
/// into equal-length segments) from publisher-supplied RSS / Podcasting 2.0
/// JSON chapters. iOS renders a `sparkles` badge for AI chapters so the
/// user can tell at a glance where the boundaries came from. Mirrors
/// `podcast_core::Chapter::is_ai_generated`; serialized always (no
/// `skip_serializing_if`) so the wire payload carries the explicit
/// `false` for RSS chapters — the iOS shell relies on the field being
/// present to render the badge or not.
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

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Narrow on purpose: the iOS shell only needs a handful of bools / strings
/// from this struct to gate UI (onboarding flow, manual-credentials banners,
/// …). Replaces the legacy in-memory `Settings` compat shim. The kernel
/// authoritative source is [`crate::store::PodcastStore::has_completed_onboarding`].
///
/// `Default` produces the fresh-install state (`has_completed_onboarding =
/// false`) so the snapshot builder can always emit a `SettingsSnapshot`
/// regardless of store-lock acquisition.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SettingsSnapshot {
    /// Whether the user has finished the iOS onboarding flow. iOS reads
    /// this from the `settings` snapshot to decide whether to present
    /// `OnboardingView`. Mutated via the `podcast.update_settings` action.
    pub has_completed_onboarding: bool,
}

impl SettingsSnapshot {
    /// Returns true when the snapshot equals `Default::default()`. Used as
    /// the `skip_serializing_if` guard on
    /// [`super::snapshot::PodcastUpdate::settings`] so the empty-state
    /// snapshot stays byte-identical to the legacy stub (D6).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One NIP-22 (kind 1111) comment surfaced via
/// [`super::snapshot::PodcastUpdate::comments`] for the
/// currently-playing episode.
///
/// The shape is intentionally narrow — id, author, body, timestamp.
/// Reply threading, reactions, and zaps live in follow-up projections.
///
/// `id` is the Nostr event id (lowercase hex). `author_npub` is the
/// bech32 encoding of the event's `pubkey` so the iOS shell can render
/// it without re-encoding. `author_name` is the cached display name
/// from NIP-01 metadata when the projection layer has one; `None`
/// means the UI should fall back to the truncated npub stub.
/// `created_at` is Unix seconds (matches NIP-01's `created_at`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct CommentSummary {
    /// Event id (lowercase hex) — stable Nostr identifier.
    pub id: String,
    /// Author bech32 (`npub1…`) — pre-encoded so iOS doesn't need a
    /// bech32 dependency to render the stub key.
    pub author_npub: String,
    /// Cached display name from the author's NIP-01 metadata, when
    /// known. `None` means the UI renders the truncated npub instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// Comment body — the raw `content` field of the kind 1111 event.
    pub content: String,
    /// Unix seconds (matches NIP-01 `created_at`).
    pub created_at: i64,
}

/// One agent-curated pick row surfaced via
/// [`super::snapshot::PodcastUpdate::picks`].
///
/// Built by the picks-projection layer (see `picks_module::AgentPicksModule`)
/// from a heuristic walk over the library: newest episodes across all
/// subscribed shows, capped per show for diversity, top-N overall. Real
/// LLM-driven picks land in a follow-up; the wire shape is forward-
/// compatible — the `pick_score` + `pick_reason` fields are populated by
/// whichever projection is in effect.
///
/// The fields are pre-resolved (podcast title + artwork denormalized from
/// `PodcastSummary`) so the iOS Home view can render the pick rail
/// without a second snapshot lookup per row.
///
/// `pick_score` is a `0.0..=1.0` confidence (1.0 = best pick); the
/// projection layer normalizes whichever signal it uses (recency rank in
/// the heuristic stub, model probability in the future LLM variant) onto
/// this range. `pick_reason` is a short, human-readable string the UI
/// renders directly (no localization in M-stub).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AgentPickSummary {
    /// Stable id of the episode the pick refers to. Matches
    /// `EpisodeSummary.id`.
    pub episode_id: String,
    pub episode_title: String,
    /// Owning podcast's id (string form of `PodcastId` UUID).
    pub podcast_id: String,
    pub podcast_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// Unix seconds.
    pub published_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// Short reason the row was selected, rendered in the chip overlay.
    /// e.g. `"New from {podcast_title}"`.
    pub pick_reason: String,
    /// `0.0..=1.0` — higher is better. Used for sort order; the
    /// projection layer is the sole owner of normalization.
    pub pick_score: f32,
}

/// One row in the agent-tasks projection surfaced via
/// [`super::snapshot::PodcastUpdate::agent_tasks`].
///
/// Mirrors a recurring or one-shot action the agent has scheduled on
/// the user's behalf (e.g. "fetch new episodes every morning",
/// "generate a briefing at 7am", "research topic X").
///
/// Per D5/D7 this is pure data: the projection is consumed by the
/// `AgentTasksView` SwiftUI list and rendered without any client-side
/// logic. The kernel-side `AgentTasksModule` owns mutation; the iOS
/// shell only dispatches actions and re-renders.
///
/// `action_namespace` + `action_body` carry the dispatch payload the
/// task should fire (e.g. `"podcast.briefings.generate"` + `"{}"`).
/// Carrying them as opaque string fields keeps the projection
/// open-ended — new agent capabilities show up as new namespace
/// strings without changing this struct.
///
/// `schedule` is a free-form string (`"daily"`, `"weekly"`, `"once"`,
/// or a cron-like expression) — interpreted by the future scheduler
/// runtime, not by the projection layer.
///
/// `next_run_at` / `last_run_at` are surfaced as optional Unix
/// timestamps (seconds since epoch). `None` means the slot is
/// undefined (not-yet-scheduled / not-yet-run). The iOS view uses
/// these to render a relative-time chip.
///
/// `status` is one of `"pending"` / `"running"` / `"completed"` /
/// `"failed"`; carried as a string for the same string-discriminator
/// reason as [`DownloadItemSnapshot::state`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentTaskSummary {
    /// Stable UUID (hyphenated string) minted by `AgentTasksModule::create`.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// e.g. `"podcast.briefings.generate"` — the action namespace
    /// `run_now` would (in a future scheduler) dispatch.
    pub action_namespace: String,
    /// JSON payload (already-encoded). Keeps the projection
    /// schema-agnostic to the receiver's action shape.
    pub action_body: String,
    /// Free-form schedule label (`"daily"`, `"weekly"`, `"once"`, or
    /// a cron-like expression). The future scheduler runtime parses
    /// this; the projection layer treats it as opaque.
    pub schedule: String,
    /// Unix seconds — next scheduled run. `None` until the scheduler
    /// computes one or for `"once"` schedules that have already run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<i64>,
    /// Unix seconds — last completed (or failed) run. `None` until
    /// the task has run at least once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<i64>,
    /// Lifecycle label: `"pending"` / `"running"` / `"completed"` /
    /// `"failed"`. Defaults to `"pending"` on `create`.
    pub status: String,
    /// `true` when the scheduler should consider this task; user can
    /// toggle via `enable` / `disable` ops without losing the row.
    pub is_enabled: bool,
}

/// One row in the RAG / vector-search projection surfaced via
/// [`super::snapshot::PodcastUpdate::knowledge_search_results`].
///
/// M6.A's `podcast-knowledge` crate owns the production chunk store +
/// hybrid ranker (KNN + BM25 + RRF + reranker). The iOS shell renders
/// from this narrow wire shape so the kernel can swap the underlying
/// implementation (current stub: case-insensitive substring match over
/// `Episode.title` + `Episode.description`; follow-up: real embedding
/// search) without breaking the host decoder.
///
/// Fields:
///
/// * `episode_id` / `episode_title` / `podcast_title` — what the row
///   labels itself with. `episode_id` is the hyphenated UUID so the
///   iOS shell can dispatch `podcast.player.play` against it.
/// * `snippet` — the relevant text excerpt (up to ~200 chars). The
///   projection layer truncates so the UI never has to.
/// * `start_secs` — position in the episode the snippet appears at,
///   when the underlying chunk has a timestamp (transcripts will;
///   description-only matches stay `None`). The iOS shell renders a
///   "seek to" button only when this is set.
/// * `relevance_score` — `0.0..=1.0`. Used by the UI to render a
///   relevance bar and (incidentally) to validate the ranker's order
///   in a regression test. The stub uses a simple "how early in the
///   text did the query land" heuristic.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeSearchResult {
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_secs: Option<f64>,
    pub relevance_score: f32,
}

/// User-defined audio clip from an episode, surfaced via
/// [`super::snapshot::PodcastUpdate::clips`].
///
/// One row per saved clip. The kernel stores the clip internally as
/// `(id, episode_id, start, end, title, created_at)`; the snapshot
/// builder joins against `PodcastStore` to fill `episode_title` /
/// `podcast_title` at projection time so a podcast / episode rename
/// is reflected immediately without rewriting the clip record.
///
/// `start_secs` / `end_secs` are absolute positions inside the episode
/// (not relative). The kernel guarantees `start_secs < end_secs` at
/// create time.
///
/// `title` is the user-given clip title (e.g. "Marcus on retrieval"),
/// distinct from `episode_title`. `None` when the user did not name
/// the clip (e.g. AutoSnip with no follow-up rename).
///
/// `created_at` is Unix seconds — matches the timestamp convention
/// already used by `PendingApprovalSnapshot::requested_at`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ClipSummary {
    pub id: String,
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    /// Clip start position in seconds, absolute within the episode.
    /// Serialized as the same `f64` Swift `Codable` decodes as
    /// `Double`.
    pub start_secs: f64,
    /// Clip end position in seconds, absolute within the episode.
    /// Must satisfy `end_secs > start_secs` (enforced at create time).
    pub end_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Unix seconds when the clip was created. Set by the kernel
    /// (`chrono::Utc::now()` in the action handler) — never by the
    /// host.
    pub created_at: i64,
}

/// Snapshot row for a podcast the user owns (has generated a NIP-F4
/// per-podcast keypair for via the `podcast.publish.create_owned_podcast`
/// action). Surfaced via [`super::snapshot::PodcastUpdate::owned_podcasts`].
///
/// `show_event_json` is the most recently constructed `kind:10154` event
/// (unsigned, for debug/diagnostic visibility) — the relay-publish path
/// is `relay_pending` until the broader Nostr publishing infrastructure
/// is wired through. `last_published_at` is Unix seconds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct OwnedPodcastInfo {
    pub podcast_id: String,
    pub podcast_pubkey_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_event_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_published_at: Option<i64>,
}

/// Narrow identity projection surfaced via
/// [`super::snapshot::PodcastUpdate::active_account`].
///
/// Present when an identity is loaded; `None` while the kernel hasn't yet
/// resolved the active account (pre-sign-in or between identity switch and
/// the first snapshot tick).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AccountSummary {
    pub npub: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_url: Option<String>,
}

/// One contact row in [`SocialSnapshot::following`] — the user's NIP-02
/// (kind:3) follow list, projected for the iOS "Social" tab.
///
/// The shape is intentionally narrow: an avatar grid only needs the bech32
/// pubkey, a display name to surface under the avatar, and the picture URL.
/// Richer profile fields (NIP-05, NIP-39 external identities, lud16, …)
/// belong on a separate profile-detail projection so the grid stays cheap
/// to decode.
///
/// `npub` is pre-encoded so the iOS shell doesn't need a bech32 dependency
/// just to render the avatar fallback (truncated key).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ContactSummary {
    /// Author bech32 (`npub1…`) — pre-encoded so iOS can render the
    /// truncated-key fallback without a bech32 dep.
    pub npub: String,
    /// Cached display name from the contact's NIP-01 metadata, when
    /// known. `None` means the grid renders the truncated npub instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Cached avatar URL from the contact's NIP-01 metadata, when known.
    /// `None` means the grid renders the initial / fallback avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_url: Option<String>,
}

/// Snapshot of the user's Nostr social graph surfaced via
/// [`super::snapshot::PodcastUpdate::social`].
///
/// Mirrors the NIP-02 contact list (kind:3 follows) that the underlying
/// NMP substrate registers via `register_defaults`. For this PR the
/// projection layer still emits `None` — the contact store hook-up is
/// tracked in `docs/BACKLOG.md` (`pr-social-graph-nmp-store-wiring`) —
/// but the shape is fixed so the iOS shell can render against it as soon
/// as the data lands.
///
/// `following_count` is provided as a sugar so the UI can render the tab
/// badge without iterating `following`; it equals `following.len()` when
/// the projection is freshly built but stays correct even when callers
/// page through `following`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SocialSnapshot {
    /// Contacts the active account is following (NIP-02 kind:3 `p` tags).
    /// Empty when the contact list has been fetched but is genuinely
    /// empty; the field is `None` (not `Some([])`) when the projection
    /// layer hasn't fetched yet — see [`super::snapshot::PodcastUpdate`].
    pub following: Vec<ContactSummary>,
    /// Number of contacts on the active follow list. Equal to
    /// `following.len()` for now; surfaced separately so paged variants
    /// of `following` keep working without a second snapshot field.
    pub following_count: usize,
/// One row in [`super::snapshot::PodcastUpdate::wiki_articles`].
///
/// A `WikiArticle` is the AI-synthesised, per-podcast knowledge entry the user
/// builds up over time. Each article is keyed by `id` (UUID) and scoped to a
/// single `podcast_id`; `topic` is the user-supplied subject line and
/// `summary` is the LLM-rendered body (1-2 paragraphs in the scaffold; real
/// synthesis is a follow-up).
///
/// `source_episode_ids` lists the episode ids the synthesis drew from, so the
/// iOS reader can render tappable provenance rows that jump to the relevant
/// episode detail screen. `last_updated_at` is unix seconds — Swift can
/// compare against `Date()` without a formatter round-trip, mirroring the
/// pattern used by [`PendingApprovalSnapshot::requested_at`].
///
/// `is_generating` is the lifecycle flag the UI flips on while a generation
/// is in flight. In the scaffold the action handler completes synchronously
/// (`is_generating = false`); the field exists so the LLM-backed follow-up
/// can mutate it without renegotiating the wire shape.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct WikiArticle {
    /// Stable UUID for the article (hyphenated). The iOS reader uses this
    /// as the `Identifiable.id` and as the argument to
    /// `podcast.wiki.delete`.
    pub id: String,
    /// Owning podcast id (matches [`PodcastSummary::id`]). Used to filter
    /// the article list down to the current show on the iOS side.
    pub podcast_id: String,
    /// User-supplied subject — what the article is *about*.
    pub topic: String,
    /// Rendered body (1-2 paragraph summary in the scaffold).
    pub summary: String,
    /// Episode ids the synthesis drew from. Empty in the scaffold —
    /// populated once the LLM follow-up wires real retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_episode_ids: Vec<String>,
    /// Unix seconds — see struct-level comment.
    pub last_updated_at: i64,
    /// `true` while a generation is in flight; `false` once the article is
    /// readable. Lets the UI render a progress indicator without polling.
    pub is_generating: bool,
/// One row in the agent-memory projection surfaced via
/// [`super::snapshot::PodcastUpdate::memory_facts`].
///
/// Agent memory is a flat key→value bag the AI agent (and the user) can
/// write to so the assistant remembers durable facts about the user across
/// sessions (`"preferred_genre"` → `"technology"`,
/// `"timezone"` → `"Europe/Madrid"`, …). Keyed on `key` so writes upsert:
/// the most recent write wins.
///
/// `source` is `"user"` when the user wrote the fact through Settings,
/// `"agent"` when the assistant recorded it mid-conversation. Surfaced as
/// a string (not a typed enum) so the iOS decoder doesn't need a variant
/// case-mapping — matches every other `source` / `status` field on the
/// snapshot.
///
/// `created_at` is Unix seconds — the rendering layer formats it; the
/// projection stays format-free.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct MemoryFact {
    /// Stable id for the row. Currently the same as `key` so two writes
    /// from the same user collapse on upsert (key is the upsert handle).
    /// Surfaced as `String` rather than typed `Uuid` so the iOS
    /// `Identifiable` conformance has a non-empty handle without any
    /// custom decoding.
    pub id: String,
    /// User-readable key (e.g. `"preferred_genre"`).
    pub key: String,
    /// Free-form value the agent or user wrote.
    pub value: String,
    /// `"user"` or `"agent"`. The action handler defaults missing values
    /// to `"user"` so the wire shape stays narrow for hand-written calls.
    pub source: String,
    /// Unix seconds when the fact was first written (preserved across
    /// upserts so the UI can show "remembered since …" if it wants to).
    pub created_at: i64,
/// One row in the agent-generated TTS episode list surfaced via
/// [`super::snapshot::PodcastUpdate::tts_episodes`].
///
/// These are not "real" podcast episodes — they live entirely in
/// kernel-side memory on the [`super::handle::PodcastHandle`], not in
/// [`crate::store::PodcastStore`], because they don't have a feed, an
/// enclosure URL, or any of the other RSS-derived fields the
/// [`EpisodeSummary`] projection carries. The script string is the
/// text that the iOS voice executor will speak when the user taps
/// "play"; the kernel mints it and never has to re-derive it.
///
/// `status` is a string discriminator (`"generating_script"` |
/// `"ready"` | `"played"`) rather than a typed enum so the Swift
/// `Codable` decoder doesn't need a case-mapping for what is purely a
/// display chip in the list.
///
/// `voice_id` is `Option` because the M0 stub generator does not pick
/// a voice — the executor falls back to its currently configured one.
/// A future LLM-script generator may choose a voice per-episode.
///
/// `Eq` is intentionally not derived because `duration_estimate_secs`
/// is `f64`; partial equality (`PartialEq`) is sufficient for snapshot
/// round-trip tests where the value goes through serde without
/// arithmetic.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TtsEpisodeSummary {
    /// Stable UUID minted by the kernel on `generate`. Rendered as
    /// the canonical hyphenated string for Swift `Identifiable`.
    pub id: String,
    pub title: String,
    /// The plain-text script that the voice capability will speak.
    /// Surfaced to the iOS list so the user can preview before
    /// tapping play (truncated by the UI as needed).
    pub script: String,
    /// Best-effort duration estimate. Computed by the kernel from the
    /// requested `length_minutes` (so generating a "5 minute" episode
    /// yields `300.0` seconds even though the placeholder script
    /// itself is much shorter). The follow-up LLM generator will
    /// replace this with an actual word-count-based estimate.
    pub duration_estimate_secs: f64,
    /// Unix seconds at the moment `generate` was dispatched.
    pub created_at: i64,
    /// One of `"generating_script"`, `"ready"`, `"played"`. The M0
    /// stub generator emits `"ready"` immediately; the future LLM
    /// generator will surface `"generating_script"` while the script
    /// is being synthesised.
    pub status: String,
    /// Optional voice id (provider-specific opaque string). `None`
    /// means "use the executor's currently configured voice."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_snapshot_omits_none_optionals() {
        // Empty widget (e.g. nothing loaded) should not pollute the JSON
        // payload with `null` strings — the widget reads it as "show empty".
        let widget = WidgetSnapshot {
            now_playing_episode_title: None,
            now_playing_podcast_title: None,
            now_playing_artwork_url: None,
            is_playing: false,
            position_fraction: 0.0,
            unplayed_count: 0,
        };
        let json = serde_json::to_string(&widget).expect("encode");
        assert!(!json.contains("now_playing_episode_title"));
        assert!(!json.contains("now_playing_podcast_title"));
        assert!(!json.contains("now_playing_artwork_url"));
        assert!(json.contains("\"is_playing\":false"));
        assert!(json.contains("\"position_fraction\":0.0"));
        assert!(json.contains("\"unplayed_count\":0"));
    }

    #[test]
    fn episode_summary_omits_none_download_path() {
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        // No download yet — field must not appear on the wire.
        assert!(!json.contains("download_path"));
    }

    #[test]
    fn episode_summary_round_trips_with_download_path() {
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            download_path: Some("/var/mobile/Containers/Downloads/ep-1.mp3".into()),
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        assert!(json.contains("download_path"));
        let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ep);
    }

    #[test]
    fn episode_summary_omits_empty_chapters() {
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        assert!(!json.contains("chapters"));
    }

    #[test]
    fn episode_summary_round_trips_with_chapters() {
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            chapters: vec![
                ChapterSummary {
                    start_secs: 0.0,
                    end_secs: Some(60.0),
                    title: "Intro".into(),
                    image_url: Some("https://ex.com/intro.png".into()),
                    url: None,
                    is_ai_generated: false,
                },
                ChapterSummary {
                    start_secs: 60.0,
                    title: "Main".into(),
                    ..ChapterSummary::default()
                },
            ],
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ep);
        assert!(!json.contains("\"url\":null"));
    }

    #[test]
    fn episode_summary_omits_none_playback_position() {
        // No resume point on a fresh episode — field stays off the wire so
        // older binaries continue to round-trip cleanly and the iOS shell
        // doesn't render a "Resume at 0:00" indicator on every untouched row.
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        assert!(!json.contains("playback_position_secs"));
    }

    #[test]
    fn episode_summary_round_trips_with_playback_position() {
        let ep = EpisodeSummary {
            id: "ep-1".into(),
            title: "Pilot".into(),
            playback_position_secs: Some(123.5),
            ..EpisodeSummary::default()
        };
        let json = serde_json::to_string(&ep).expect("encode");
        assert!(json.contains("\"playback_position_secs\":123.5"));
        let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ep);
    }

    #[test]
    fn settings_snapshot_round_trips() {
        let s = SettingsSnapshot { has_completed_onboarding: true };
        let json = serde_json::to_string(&s).expect("encode");
        assert!(json.contains("\"has_completed_onboarding\":true"));
        let decoded: SettingsSnapshot = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, s);
    }

    #[test]
    fn settings_snapshot_default_is_fresh_install() {
        let s = SettingsSnapshot::default();
        assert!(!s.has_completed_onboarding);
        let json = serde_json::to_string(&s).expect("encode");
        assert!(json.contains("\"has_completed_onboarding\":false"));
    fn comment_summary_omits_none_author_name() {
        // Anonymous (or yet-uncached) author — `author_name` must not
        // appear in the JSON, so iOS reliably falls back to the npub stub.
        let c = CommentSummary {
            id: "abc".into(),
            author_npub: "npub1example".into(),
            author_name: None,
            content: "first!".into(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&c).expect("encode");
        assert!(!json.contains("author_name"));
        let decoded: CommentSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, c);
    }

    #[test]
    fn comment_summary_round_trips_with_author_name() {
        let c = CommentSummary {
            id: "abc".into(),
            author_npub: "npub1example".into(),
            author_name: Some("Satoshi".into()),
            content: "love this episode".into(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&c).expect("encode");
        assert!(json.contains("\"author_name\":\"Satoshi\""));
        let decoded: CommentSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, c);
    fn chapter_summary_ai_generated_round_trip() {
        let ai = ChapterSummary {
            start_secs: 0.0,
            title: "Chapter 1".into(),
            is_ai_generated: true,
            ..ChapterSummary::default()
        };
        let json = serde_json::to_string(&ai).expect("encode");
        assert!(json.contains("\"is_ai_generated\":true"));
        let decoded: ChapterSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ai);
    }

    #[test]
    fn chapter_summary_decodes_when_is_ai_generated_omitted() {
        // Forward-compat: payloads predating the field decode with `is_ai_generated = false`.
        let json = r#"{"start_secs":0.0,"title":"Intro"}"#;
        let decoded: ChapterSummary = serde_json::from_str(json).expect("decode");
        assert!(!decoded.is_ai_generated);
    fn agent_task_summary_round_trips_with_all_fields() {
        let task = AgentTaskSummary {
            id: "task-1".into(),
            title: "Morning Briefing".into(),
            description: Some("Generate a briefing every morning".into()),
            action_namespace: "podcast.briefings.generate".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
            next_run_at: Some(1_700_000_000),
            last_run_at: Some(1_699_900_000),
            status: "completed".into(),
            is_enabled: true,
        };
        let json = serde_json::to_string(&task).expect("encode");
        let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, task);
    }

    #[test]
    fn agent_task_summary_omits_none_optionals() {
        let task = AgentTaskSummary {
            id: "task-1".into(),
            title: "Inbox Triage".into(),
            description: None,
            action_namespace: "podcast.inbox.triage".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
            next_run_at: None,
            last_run_at: None,
            status: "pending".into(),
            is_enabled: true,
        };
        let json = serde_json::to_string(&task).expect("encode");
        assert!(!json.contains("description"));
        assert!(!json.contains("next_run_at"));
        assert!(!json.contains("last_run_at"));
        // Round-trip survives the elided optionals.
        let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, task);
    fn knowledge_search_result_round_trips_with_all_fields() {
        let row = KnowledgeSearchResult {
            episode_id: "ep-1".into(),
            episode_title: "Pilot".into(),
            podcast_title: "Some Show".into(),
            snippet: "…the relevant excerpt…".into(),
            start_secs: Some(123.5),
            relevance_score: 0.87,
        };
        let json = serde_json::to_string(&row).expect("encode");
        assert!(json.contains("\"start_secs\":123.5"));
        let decoded: KnowledgeSearchResult = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, row);
    }

    #[test]
    fn knowledge_search_result_omits_none_start_secs() {
        let row = KnowledgeSearchResult {
            episode_id: "ep-1".into(),
            episode_title: "Pilot".into(),
            podcast_title: "Some Show".into(),
            snippet: "x".into(),
            start_secs: None,
            relevance_score: 0.5,
        };
        let json = serde_json::to_string(&row).expect("encode");
        assert!(!json.contains("start_secs"));
        let decoded: KnowledgeSearchResult = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, row);
    fn clip_summary_omits_none_title() {
        let clip = ClipSummary {
            id: "clip-1".into(),
            episode_id: "ep-1".into(),
            episode_title: "Pilot".into(),
            podcast_title: "Some Show".into(),
            start_secs: 10.0,
            end_secs: 70.0,
            title: None,
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&clip).expect("encode");
        assert!(!json.contains("\"title\""));
        let decoded: ClipSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, clip);
    }

    #[test]
    fn clip_summary_round_trips_with_title() {
        let clip = ClipSummary {
            id: "clip-1".into(),
            episode_id: "ep-1".into(),
            episode_title: "Pilot".into(),
            podcast_title: "Some Show".into(),
            start_secs: 12.5,
            end_secs: 72.5,
            title: Some("Marcus on retrieval".into()),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&clip).expect("encode");
        assert!(json.contains("\"title\":\"Marcus on retrieval\""));
        assert!(json.contains("\"start_secs\":12.5"));
        assert!(json.contains("\"end_secs\":72.5"));
        let decoded: ClipSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, clip);
    }

    #[test]
    fn widget_snapshot_round_trips_with_all_fields() {
        let widget = WidgetSnapshot {
            now_playing_episode_title: Some("Ep 42".into()),
            now_playing_podcast_title: Some("Some Show".into()),
            now_playing_artwork_url: Some("https://ex.com/art.png".into()),
            is_playing: true,
            position_fraction: 0.42,
            unplayed_count: 7,
        };
        let json = serde_json::to_string(&widget).expect("encode");
        let decoded: WidgetSnapshot = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, widget);
    }

    #[test]
    fn contact_summary_omits_none_optionals() {
        // Pre-fetch contact rows may only have the npub — the optional
        // metadata fields must not pollute the wire payload.
        let c = ContactSummary {
            npub: "npub1example".into(),
            display_name: None,
            picture_url: None,
        };
        let json = serde_json::to_string(&c).expect("encode");
        assert!(!json.contains("display_name"));
        assert!(!json.contains("picture_url"));
        let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, c);
    }

    #[test]
    fn contact_summary_round_trips_with_metadata() {
        let c = ContactSummary {
            npub: "npub1example".into(),
            display_name: Some("Satoshi".into()),
            picture_url: Some("https://ex.com/avatar.png".into()),
        };
        let json = serde_json::to_string(&c).expect("encode");
        let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, c);
    }

    #[test]
    fn social_snapshot_round_trips_with_contacts() {
        let snap = SocialSnapshot {
            following: vec![
                ContactSummary {
                    npub: "npub1aaa".into(),
                    display_name: Some("Alice".into()),
                    picture_url: None,
                },
                ContactSummary {
                    npub: "npub1bbb".into(),
                    display_name: None,
                    picture_url: Some("https://ex.com/b.png".into()),
                },
            ],
            following_count: 2,
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: SocialSnapshot = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, snap);
    }

    #[test]
    fn social_snapshot_default_is_empty() {
        // Default projection (post-fetch but genuinely empty follow list)
        // must serialize cleanly without optional bloat.
        let snap = SocialSnapshot::default();
        let json = serde_json::to_string(&snap).expect("encode");
        assert!(json.contains("\"following\":[]"));
        assert!(json.contains("\"following_count\":0"));
    // ── Wiki article (#39 — AI wiki scaffold) ────────────────────────
    //
    // Round-trip coverage lives in `super::super::snapshot_tests` because
    // the WikiArticle is only ever encountered via `PodcastUpdate`. The
    // assertion that an empty `source_episode_ids` is omitted from the
    // wire payload (D5) is co-located here to keep the contract close to
    // the struct definition.
    #[test]
    fn wiki_article_omits_empty_sources_on_wire() {
        let article = WikiArticle {
            id: "art-1".into(),
            podcast_id: "pod-1".into(),
            topic: "Bitcoin halvings".into(),
            summary: "Stub summary.".into(),
            source_episode_ids: vec![],
            last_updated_at: 1_700_000_000,
            is_generating: false,
        };
        let json = serde_json::to_string(&article).expect("encode");
        assert!(!json.contains("source_episode_ids"));
    // ── AgentPickSummary projection (feature #46) ───────────────────

    #[test]
    fn agent_pick_summary_round_trips_with_all_fields() {
        let pick = AgentPickSummary {
            episode_id: "ep-1".into(),
            episode_title: "Pilot".into(),
            podcast_id: "pod-1".into(),
            podcast_title: "Some Show".into(),
            artwork_url: Some("https://ex.com/art.png".into()),
            published_at: 1_700_000_000,
            duration_secs: Some(3600.0),
            pick_reason: "New from Some Show".into(),
            pick_score: 0.95,
        };
        let json = serde_json::to_string(&pick).expect("encode");
        let decoded: AgentPickSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, pick);
    }

    #[test]
    fn agent_pick_summary_omits_none_optionals() {
        let pick = AgentPickSummary {
            episode_id: "ep-2".into(),
            episode_title: "Untitled".into(),
            podcast_id: "pod-2".into(),
            podcast_title: "No-Art Show".into(),
            artwork_url: None,
            published_at: 1_700_000_000,
            duration_secs: None,
            pick_reason: "New".into(),
            pick_score: 0.5,
        };
        let json = serde_json::to_string(&pick).expect("encode");
        assert!(!json.contains("artwork_url"));
        assert!(!json.contains("duration_secs"));
        let decoded: AgentPickSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, pick);
    fn memory_fact_round_trips() {
        let fact = MemoryFact {
            id: "preferred_genre".into(),
            key: "preferred_genre".into(),
            value: "technology".into(),
            source: "user".into(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&fact).expect("encode");
        let decoded: MemoryFact = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, fact);
    }

    #[test]
    fn memory_fact_decodes_agent_source() {
        // The agent writes facts with source="agent" — same wire shape,
        // just a different `source` literal. Decoder accepts both.
        let json = r#"{"id":"k","key":"k","value":"v","source":"agent","created_at":1700000000}"#;
        let decoded: MemoryFact = serde_json::from_str(json).expect("decode");
        assert_eq!(decoded.source, "agent");
    fn tts_episode_summary_round_trips_with_all_fields() {
        let ep = TtsEpisodeSummary {
            id: "tts-1".into(),
            title: "AI Roundup".into(),
            script: "Hello, this is your daily roundup.".into(),
            duration_estimate_secs: 300.0,
            created_at: 1_700_000_000,
            status: "ready".into(),
            voice_id: Some("rachel".into()),
        };
        let json = serde_json::to_string(&ep).expect("encode");
        let decoded: TtsEpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ep);
    }

    #[test]
    fn tts_episode_summary_omits_none_voice_id() {
        let ep = TtsEpisodeSummary {
            id: "tts-1".into(),
            title: "Generated".into(),
            script: "hi".into(),
            duration_estimate_secs: 60.0,
            created_at: 0,
            status: "ready".into(),
            voice_id: None,
        };
        let json = serde_json::to_string(&ep).expect("encode");
        assert!(!json.contains("voice_id"));
        let decoded: TtsEpisodeSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, ep);
    }
    // InboxItem round-trip / wire-shape tests live next to
    // `inbox_handler::build_inbox` in `crate::inbox_handler::tests` so
    // they stay near the projection layer that builds them.
}

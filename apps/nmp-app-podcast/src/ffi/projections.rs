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
/// * `segment_count` — number of editorial segments produced (0
///   until `status == "ready"`).
/// * `next_scheduled_minutes` — minutes until the next scheduled
///   briefing today, when the scheduler has an active schedule that
///   covers today and the slot hasn't passed yet.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSnapshot {
    /// One of `"pending"`, `"generating"`, `"ready"`, `"delivered"`,
    /// `"failed"` — matches `podcast_briefings::BriefingStatus::label`.
    pub status: String,
    /// Number of editorial segments in the active briefing. Zero
    /// until the composer completes.
    pub segment_count: usize,
    /// Minutes until the next scheduled briefing slot on the current
    /// calendar day. `None` when no schedule is active, when today
    /// isn't covered, or when the slot has already passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_scheduled_minutes: Option<u32>,
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
    /// Plain-text transcript for the episode, when one has been fetched.
    ///
    /// Populated by the snapshot builder from `PodcastStore::transcript_for`.
    /// `None` when the user has not yet dispatched `podcast.fetch_transcript`
    /// for this episode, or when the most recent fetch produced no usable
    /// text (no publisher URL, parse failure, HTTP error). The iOS shell
    /// renders the "not available" state in those cases. Per D5 we skip
    /// serializing `None` so the wire payload stays byte-compatible with
    /// snapshots that predate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    /// Narrow chapter rows projected from `podcast_core::Episode::chapters`
    /// after a `podcast.fetch_chapters` action lands. Empty when the episode
    /// has no chapter markers, or when chapters have not been fetched yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<ChapterSummary>,
}

/// Narrow chapter projection for the player rail. Mirrors the relevant
/// fields of `podcast_core::Chapter` for UI rendering.
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
}

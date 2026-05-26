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

//! Per-episode download record + its lifecycle state.
//!
//! Kept in a separate file from [`super::DownloadQueue`] so the state-machine
//! logic stays close to its doc-comment narrative and so editing the queue
//! algorithm doesn't churn the file with the public-ish types the rest of
//! the kernel reads from.

use serde::{Deserialize, Serialize};

/// Lifecycle state for a single [`DownloadItem`].
///
/// Transitions are driven by [`super::DownloadQueue`] in response to
/// [`crate::capability::DownloadReport`] events; iOS never inspects this
/// enum (D7).
///
/// ```text
///                  enqueue (slot free)
///                ┌─────────────────────────┐
///                v                         │
///   ┌────────┐ enqueue ┌────────┐ Progress │
///   │ Queued ├────────►│ Active │──────────┘
///   └────┬───┘  (slot  └──┬──┬──┘
///        │   full)        │  │
///        │ cancel/        │  │ Completed
///        │ slot frees     │  ▼
///        │            ┌───┴──────┐
///        │            │ Completed│
///        │            └──────────┘
///        │                │
///        │                │ Failed
///        │                ▼
///        │            ┌────────┐
///        │            │ Failed │
///        │            └────────┘
///        │
///        └──cancel──► ┌──────────┐
///                     │ Cancelled│
///                     └──────────┘
/// ```
///
/// A `PauseDownload` command moves Active → Paused; `ResumeDownload` moves
/// Paused → Active. Paused holds a concurrency slot — Resume is expected
/// shortly. If the user wants to free the slot for another download they
/// must explicitly Cancel.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadItemState {
    /// Waiting for a concurrency slot to free up. No `StartDownload`
    /// command has been emitted for this item yet.
    #[default]
    Queued,
    /// `StartDownload` was emitted; the iOS executor is fetching bytes
    /// and emitting `Progress` reports.
    Active,
    /// `PauseDownload` was emitted (or the executor reported `Paused`
    /// in response). Holds the concurrency slot.
    Paused,
    /// `Completed` report received. Terminal — does not free a slot
    /// retroactively, but new `enqueue` calls won't see it as active.
    Completed,
    /// `Failed` report received. Terminal — retry policy lives in
    /// `podcast-feeds::refresh::policy` (M4.B), not here.
    Failed,
    /// `Cancelled` report received (or the queue cancelled while still
    /// `Queued`). Terminal.
    Cancelled,
}

impl DownloadItemState {
    /// `true` iff this state holds a concurrency slot (Active or Paused).
    ///
    /// Used by [`super::DownloadQueue::active_count`] to bound the number
    /// of in-flight downloads against `max_concurrent`.
    #[must_use]
    pub fn holds_slot(self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }

    /// `true` iff this state is terminal (Completed, Failed, Cancelled).
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// One entry in the [`super::DownloadQueue`] — the per-episode record the
/// queue mutates in response to capability reports.
///
/// Fields are `pub` because the queue is the sole writer (D4) and callers
/// outside the queue only read this for snapshot projection.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DownloadItem {
    /// Stable episode id. Mirrors the `episode_id` field every
    /// `DownloadCommand` / `DownloadReport` variant carries.
    pub episode_id: String,
    /// HTTP/HTTPS URL of the enclosure to fetch. The queue keeps it so
    /// it can re-emit `StartDownload` on resume / re-queue without a
    /// separate lookup against the episode store.
    pub url: String,
    /// Current lifecycle state.
    pub state: DownloadItemState,
    /// Bytes downloaded so far (from the most recent `Progress` /
    /// `Paused` report). Zero until the first report arrives.
    pub bytes_downloaded: u64,
    /// Authoritative total bytes once the server reports `Content-Length`.
    /// `None` before the first `Progress` report with a known total.
    pub total_bytes: Option<u64>,
    /// On-disk path the executor wrote the completed file to. `None`
    /// until a `Completed` report lands.
    pub local_path: Option<String>,
    /// Most recent failure diagnostic (from `Failed.error`). Set only
    /// while `state == Failed`.
    pub error: Option<String>,
}

impl DownloadItem {
    /// Construct a fresh `Queued` item.
    #[must_use]
    pub fn queued(episode_id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            episode_id: episode_id.into(),
            url: url.into(),
            state: DownloadItemState::Queued,
            bytes_downloaded: 0,
            total_bytes: None,
            local_path: None,
            error: None,
        }
    }

    /// Progress in `0.0..=1.0`, or `0.0` when `total_bytes` is unknown.
    ///
    /// Surfaced via the snapshot projection so the UI can render a
    /// determinate progress bar when possible and an indeterminate
    /// spinner otherwise.
    #[must_use]
    pub fn progress_fraction(&self) -> f32 {
        match self.total_bytes {
            Some(total) if total > 0 => {
                let frac = (self.bytes_downloaded as f64) / (total as f64);
                frac.clamp(0.0, 1.0) as f32
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_slot_only_for_active_and_paused() {
        assert!(DownloadItemState::Active.holds_slot());
        assert!(DownloadItemState::Paused.holds_slot());
        assert!(!DownloadItemState::Queued.holds_slot());
        assert!(!DownloadItemState::Completed.holds_slot());
        assert!(!DownloadItemState::Failed.holds_slot());
        assert!(!DownloadItemState::Cancelled.holds_slot());
    }

    #[test]
    fn is_terminal_only_for_completed_failed_cancelled() {
        assert!(!DownloadItemState::Queued.is_terminal());
        assert!(!DownloadItemState::Active.is_terminal());
        assert!(!DownloadItemState::Paused.is_terminal());
        assert!(DownloadItemState::Completed.is_terminal());
        assert!(DownloadItemState::Failed.is_terminal());
        assert!(DownloadItemState::Cancelled.is_terminal());
    }

    #[test]
    fn progress_fraction_handles_unknown_total() {
        let item = DownloadItem::queued("ep-1", "https://ex.com/a.mp3");
        assert_eq!(item.progress_fraction(), 0.0);
    }

    #[test]
    fn progress_fraction_clamps_to_unit() {
        let mut item = DownloadItem::queued("ep-1", "https://ex.com/a.mp3");
        item.bytes_downloaded = 1000;
        item.total_bytes = Some(500); // pathological — clamp.
        assert_eq!(item.progress_fraction(), 1.0);
    }

    #[test]
    fn progress_fraction_half_when_half_done() {
        let mut item = DownloadItem::queued("ep-1", "https://ex.com/a.mp3");
        item.bytes_downloaded = 50;
        item.total_bytes = Some(100);
        assert!((item.progress_fraction() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn item_round_trips_through_serde() {
        let mut item = DownloadItem::queued("ep-1", "https://ex.com/a.mp3");
        item.state = DownloadItemState::Active;
        item.bytes_downloaded = 1024;
        item.total_bytes = Some(8192);
        let json = serde_json::to_string(&item).expect("encode");
        let decoded: DownloadItem = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, item);
    }
}

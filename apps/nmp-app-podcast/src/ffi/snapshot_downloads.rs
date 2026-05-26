//! Download-queue projection helper for [`super::snapshot::build_snapshot_payload`].
//!
//! Keeps queue-state presentation in one place: the runtime
//! [`crate::download::DownloadQueue`] remains the writer, while this module
//! turns its active/queued/paused/failed rows into the FFI snapshot shape the
//! Swift UI reads.

use crate::download::{DownloadItemState, DownloadQueue};

use super::projections::{DownloadItemSnapshot, DownloadQueueSnapshot};

pub(super) fn build_downloads_snapshot(queue: &DownloadQueue) -> Option<DownloadQueueSnapshot> {
    let mut active: Vec<DownloadItemSnapshot> = queue
        .items
        .values()
        .filter(|item| !item.state.is_terminal() || item.state == DownloadItemState::Failed)
        .map(|item| DownloadItemSnapshot {
            episode_id: item.episode_id.clone(),
            progress: item.progress_fraction(),
            state: state_label(item.state).to_owned(),
            error: item.error.clone(),
        })
        .collect();

    if active.is_empty() {
        return None;
    }

    active.sort_by_key(|item| match item.state.as_str() {
        "active" => 0u8,
        "paused" => 1,
        "queued" => 2,
        "failed" => 3,
        _ => 4,
    });

    Some(DownloadQueueSnapshot {
        queued_count: queue.queued_count(),
        active,
        completed_today: 0,
    })
}

fn state_label(state: DownloadItemState) -> &'static str {
    match state {
        DownloadItemState::Queued => "queued",
        DownloadItemState::Active => "active",
        DownloadItemState::Paused => "paused",
        DownloadItemState::Completed => "completed",
        DownloadItemState::Failed => "failed",
        DownloadItemState::Cancelled => "cancelled",
    }
}

#[cfg(test)]
#[path = "snapshot_downloads_tests.rs"]
mod tests;

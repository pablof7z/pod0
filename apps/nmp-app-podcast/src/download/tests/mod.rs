//! Unit tests for [`super::DownloadQueue`].
//!
//! Split by concern so each module stays comfortably under the 300-LOC
//! soft cap:
//!
//! * [`enqueue`] — enqueue + concurrency-cap behaviour and idempotence.
//! * [`lifecycle`] — `handle_report` projection for each variant and the
//!   slot-frees-up follow-up command logic.
//! * [`pause_resume_cancel`] — pause/resume/cancel/cancel-all semantics.

mod enqueue;
mod lifecycle;
mod pause_resume_cancel;

use super::*;
use crate::capability::DownloadCommand;

/// Helper: pull the `episode_id` out of a `StartDownload` command for the
/// tests that just want to assert "this id started next".
pub(super) fn start_id(cmd: &DownloadCommand) -> Option<&str> {
    match cmd {
        DownloadCommand::StartDownload { episode_id, .. } => Some(episode_id.as_str()),
        _ => None,
    }
}

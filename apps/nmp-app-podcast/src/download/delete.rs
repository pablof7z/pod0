//! Filesystem deletion helpers for downloaded episode files.
//!
//! Rust owns the durable download state, but the filesystem can still reject a
//! delete. These helpers keep the result handling consistent across explicit
//! "delete download" actions and auto-delete-after-played policy paths.

use std::io::ErrorKind;

use podcast_core::EpisodeId;

use crate::store::events::{stage, EventDetail, EventSeverity};
use crate::store::PodcastStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DownloadFileDeleteOutcome {
    Removed,
    AlreadyMissing,
    Failed(String),
}

pub(crate) fn remove_download_file(path: &str) -> DownloadFileDeleteOutcome {
    match std::fs::remove_file(path) {
        Ok(()) => DownloadFileDeleteOutcome::Removed,
        Err(err) if err.kind() == ErrorKind::NotFound => DownloadFileDeleteOutcome::AlreadyMissing,
        Err(err) => DownloadFileDeleteOutcome::Failed(err.to_string()),
    }
}

pub(crate) fn record_download_delete_success(
    store: &mut PodcastStore,
    episode_id_str: &str,
    episode_id: EpisodeId,
    summary: &str,
) -> bool {
    let cleared = store.clear_local_path(&episode_id).is_some();
    if cleared {
        store.emit_event_simple(
            episode_id_str,
            stage::DOWNLOAD_DELETED,
            EventSeverity::Info,
            summary,
        );
    }
    cleared
}

pub(crate) fn record_download_delete_failure(
    store: &mut PodcastStore,
    episode_id: &str,
    path: &str,
    error: &str,
) {
    store.emit_event(
        episode_id,
        stage::DOWNLOAD_DELETE_FAILED,
        EventSeverity::Failure,
        "Download deletion failed",
        vec![
            EventDetail::new("File", path.to_owned()),
            EventDetail::new("Error", error.to_owned()),
        ],
    );
}

pub(crate) fn apply_auto_delete_download(
    store: &mut PodcastStore,
    episode_id: &str,
    success_summary: &str,
) {
    let Some((typed_id, path)) = store.auto_delete_download_candidate(episode_id) else {
        return;
    };
    match remove_download_file(&path) {
        DownloadFileDeleteOutcome::Removed | DownloadFileDeleteOutcome::AlreadyMissing => {
            record_download_delete_success(store, episode_id, typed_id, success_summary);
        }
        DownloadFileDeleteOutcome::Failed(error) => {
            record_download_delete_failure(store, episode_id, &path, &error);
        }
    }
}

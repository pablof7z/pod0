//! Pure JSON ↔ JSON bridge between the iOS audio capability and the
//! Rust [`crate::player::PlayerActor`].
//!
//! This is the seam M3.B will plug into the kernel-side `ActionModule`
//! and `CapabilityModule` registrations. Today it isolates the JSON
//! envelope handling from the actor itself so:
//!
//! 1. The actor stays a pure state machine (`PlayerActor::handle_audio_report`
//!    takes a typed `AudioReport`, not a string), keeping the unit tests
//!    cheap and the surface narrow.
//! 2. The kernel-side `ActionModule` (M3.B) and the iOS-side
//!    `PodcastCapabilities.handleJSON` router will all funnel through
//!    these helpers so the JSON shapes don't drift across the four
//!    layers (Swift encoder → C-ABI → Rust decoder → projection).
//!
//! D7 holds at every step: the helpers parse, project, and re-encode;
//! they never inspect content to make a playback decision. All decisions
//! live in [`crate::player::PlayerActor`].

use std::time::SystemTime;

use crate::capability::{AudioCommand, AudioReport, DownloadCommand, DownloadReport};
use crate::download::DownloadQueue;
use crate::player::PlayerActor;
use crate::store::PodcastStore;

/// Outcome of feeding a JSON-encoded [`AudioReport`] into a
/// [`PlayerActor`].
#[derive(Debug)]
pub enum DispatchOutcome {
    /// The report decoded and projected; `follow_up_json` is the JSON
    /// of the [`AudioCommand`] the kernel should hand back to the
    /// capability (`None` when no command is needed).
    Ok { follow_up_json: Option<String> },
    /// The inbound JSON couldn't be decoded as an [`AudioReport`].
    /// Per D6 this is data, not an exception — the caller decides
    /// whether to log, drop, or surface to diagnostics.
    DecodeFailed { error: String },
}

/// Decode a JSON-encoded [`AudioReport`], apply it to `actor`, and
/// return the follow-up [`AudioCommand`] (if any) as JSON ready to send
/// back to the iOS capability.
///
/// Errors degrade to [`DispatchOutcome::DecodeFailed`] — D6: no panics,
/// no `Result` leaking across the layer boundary in a position where the
/// caller can't recover.
pub fn dispatch_audio_report_json(
    actor: &mut PlayerActor,
    report_json: &str,
    now: SystemTime,
) -> DispatchOutcome {
    let report: AudioReport = match serde_json::from_str(report_json) {
        Ok(r) => r,
        Err(err) => {
            return DispatchOutcome::DecodeFailed {
                error: err.to_string(),
            }
        }
    };

    let follow_up = actor.handle_audio_report(report, now);
    let follow_up_json = follow_up.and_then(|cmd| serde_json::to_string(&cmd).ok());
    DispatchOutcome::Ok { follow_up_json }
}

/// Encode an [`AudioCommand`] for the iOS capability. Returns `None`
/// on the (impossible) serde failure — the caller treats `None` as
/// "no-op", which is the safest D6 fall-back for an outbound command.
#[must_use]
pub fn encode_audio_command(cmd: &AudioCommand) -> Option<String> {
    serde_json::to_string(cmd).ok()
}

// ── DownloadReport dispatch ─────────────────────────────────────────────────

/// Decode a JSON-encoded [`DownloadReport`] and project it into `store`.
///
/// **D7:** the report is an *observation* of what the iOS background
/// `URLSession` did — never an invitation for Rust to decide something.
/// The kernel projects the report into [`PodcastStore::local_paths`].
///
/// Today the projection is narrowly scoped:
///   * `Completed { local_path }` — records the on-disk path so
///     [`crate::ffi::EpisodeSummary::download_path`] becomes non-null
///     on the next snapshot.
///   * `Cancelled` — clears the local path.
///   * Every other variant (`Progress`, `Failed`, `Paused`) decodes
///     cleanly and resolves to `DispatchOutcome::Ok` with no store
///     mutation. Use [`dispatch_download_report_json_with_queue`] when
///     the caller also owns the runtime [`DownloadQueue`].
///
/// The return shape mirrors [`dispatch_audio_report_json`] so the FFI
/// shim can stay symmetric; `follow_up_json` is always `None` today.
/// Per D6, malformed JSON degrades to [`DispatchOutcome::DecodeFailed`]
/// rather than panicking across the FFI boundary.
pub fn dispatch_download_report_json(
    store: &mut PodcastStore,
    report_json: &str,
) -> DispatchOutcome {
    let report: DownloadReport = match serde_json::from_str(report_json) {
        Ok(r) => r,
        Err(err) => {
            return DispatchOutcome::DecodeFailed {
                error: err.to_string(),
            }
        }
    };
    apply_download_report(store, &report);
    DispatchOutcome::Ok {
        follow_up_json: None,
    }
}

/// Outcome of the queue-aware download dispatch.
///
/// Distinct from [`DispatchOutcome`] because the download FFI needs one extra
/// bit the audio/voice paths don't: whether the report changed *durable*
/// library state. Only a completed or cancelled download flips an
/// `Episode.downloadState` (it touches [`PodcastStore`] local paths); a
/// `Progress`/`Paused`/`Failed` tick changes only transient queue state. The
/// FFI bumps the global snapshot `rev` *only* when `durable_changed` is set, so
/// the ~1 Hz progress stream no longer forces Swift to pull + JSON-decode the
/// entire library snapshot on the main thread.
#[derive(Debug)]
pub struct DownloadDispatch {
    /// JSON of the follow-up [`DownloadCommand`] when the report freed a
    /// queue slot; `None` otherwise.
    pub follow_up_json: Option<String>,
    /// `true` when the report mutated durable store state
    /// (`Completed`/`Cancelled` that resolved to a known episode).
    pub durable_changed: bool,
    /// `true` when the inbound JSON failed to decode. Per D6 the FFI treats
    /// this as "degrade silently" (return NULL), never a panic.
    pub decode_failed: bool,
}

/// Decode a JSON-encoded [`DownloadReport`], project it into both
/// `store` and `queue`, and report whether durable library state changed
/// alongside the next queued [`DownloadCommand`] (when the report frees a
/// slot).
pub fn dispatch_download_report_json_with_queue(
    store: &mut PodcastStore,
    queue: &mut DownloadQueue,
    report_json: &str,
) -> DownloadDispatch {
    let report: DownloadReport = match serde_json::from_str(report_json) {
        Ok(r) => r,
        Err(_) => {
            return DownloadDispatch {
                follow_up_json: None,
                durable_changed: false,
                decode_failed: true,
            }
        }
    };
    let durable_changed = apply_download_report(store, &report);
    let follow_up_json = queue
        .handle_report(report)
        .into_iter()
        .next()
        .and_then(|cmd: DownloadCommand| serde_json::to_string(&cmd).ok());
    DownloadDispatch {
        follow_up_json,
        durable_changed,
        decode_failed: false,
    }
}

/// Pure projection of a typed [`DownloadReport`] onto `store`. Split out
/// so unit tests don't have to round-trip through JSON.
///
/// Returns `true` when the report mutated durable store state (a
/// `Completed`/`Cancelled` that resolved to a known episode), so the caller
/// can decide whether the change warrants a full snapshot `rev` bump.
fn apply_download_report(store: &mut PodcastStore, report: &DownloadReport) -> bool {
    match report {
        DownloadReport::Completed {
            episode_id,
            local_path,
        } => {
            if let Some((typed_id, _url)) = store.episode_enclosure_url(&episode_id) {
                // Stat the finished file once, here on the actor thread, so the
                // main-thread snapshot projection reads a cached size instead of
                // re-statting every downloaded episode on every tick. `0` when
                // the file is unreadable — the projection treats it as unknown.
                let byte_count = std::fs::metadata(local_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);
                store.set_local_path(typed_id, local_path.clone(), byte_count);
                true
            } else {
                // Episode not in the store (e.g. unsubscribed mid-flight):
                // drop the report on the floor. D6 — data, not exception.
                false
            }
        }
        DownloadReport::Cancelled { episode_id } => {
            if let Some((typed_id, _url)) = store.episode_enclosure_url(&episode_id) {
                let _ = store.clear_local_path(&typed_id);
                true
            } else {
                false
            }
        }
        DownloadReport::Failed { .. }
        | DownloadReport::Paused { .. }
        | DownloadReport::Progress { .. } => false,
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

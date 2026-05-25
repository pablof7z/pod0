//! Podcast-local download capability contract — `nmp.download.capability`.
//!
//! This is the schema the iOS executor (`Capabilities/DownloadCapability.swift`,
//! landing in M4.C) implements and the Rust [`crate::download::DownloadQueue`]
//! drives. Rust serializes a [`DownloadCommand`]; iOS executes it against a
//! background `URLSession` and sends a [`DownloadReport`] back.
//!
//! ## Doctrine
//!
//! * **D7 — capabilities report, never decide.** iOS downloads exactly what
//!   Rust tells it to download and reports exactly what happens. It never
//!   decides which queued item starts next on `Completed`/`Failed`/`Cancelled`,
//!   never decides whether to retry on `Failed`, never decides whether to
//!   honour a per-subscription auto-download policy. Queue order, concurrency
//!   cap, retry behaviour, and policy evaluation all live in
//!   [`crate::download::DownloadQueue`] and `podcast-feeds::refresh::policy`
//!   (M4.B).
//! * **D4 — single writer.** The Rust-side `DownloadQueue` is the sole writer
//!   of download state. iOS holds only `URLSessionDownloadTask`s and the maps
//!   needed to associate them with `episode_id`s; the projection that the UI
//!   reads comes from Rust.
//! * **D6 — error envelopes.** `Failed` carries an `error: String` payload;
//!   the capability never throws across the FFI.
//!
//! ## Namespace
//!
//! The namespace string is `nmp.download.capability` to match
//! `HttpCapability::namespace` / `KeychainCapability` convention and the
//! canonical capability spec at `Plans/nmp-migration/03-capabilities.md` §5.2.
//!
//! ## Schema stability
//!
//! This is the M4.A skeleton — a podcast-local two-enum Command/Report shape.
//! The canonical `nmp-core::capability::download` per `03-capabilities.md`
//! §5.2 uses a three-enum `Request`/`Response`/`Event` split with task ids,
//! `dest_path`, `etag`, and `if_modified_since` for conditional fetches.
//! When that lands in `nostrmultiplatform`, M4.B/C will reconcile this
//! contract against the canonical one in a follow-up migration. The split
//! here is deliberately narrower so the iOS executor in M4.C has a stable
//! target to implement *now* without blocking on the cross-repo dependency.
//! In particular, dest_path is left to the capability (legacy iOS uses
//! `Application Support/Downloads/<episode_id>.mp3`); etag/if-modified-since
//! land alongside resume-token support in M4.A's follow-up.

use serde::{Deserialize, Serialize};

/// Capability namespace string. Mirrors `HttpCapability::namespace` /
/// `KeyringCapability::NAMESPACE` so the iOS-side router can dispatch by
/// the same string the broader capability plan uses.
pub const DOWNLOAD_CAPABILITY_NAMESPACE: &str = "nmp.download.capability";

// ---------------------------------------------------------------------------
// Rust → iOS: DownloadCommand
// ---------------------------------------------------------------------------

/// Commands Rust dispatches to the iOS download capability.
///
/// Wire form is `serde`-tagged on `"type"` (`snake_case`):
///
/// ```text
/// {"type":"start_download","url":"…","episode_id":"…","expected_bytes":12345}
/// {"type":"pause_download","episode_id":"…"}
/// {"type":"resume_download","episode_id":"…"}
/// {"type":"cancel_download","episode_id":"…"}
/// {"type":"cancel_all"}
/// ```
///
/// **D7:** these are *imperative* actions on the background `URLSession`;
/// the iOS side runs each one against a `URLSessionDownloadTask` and
/// reports the resulting progress / completion. There is no `decide`-flavoured
/// command — every variant maps to a concrete `URLSession` call.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadCommand {
    /// Begin downloading `url` for `episode_id`. The executor creates a
    /// `URLSessionDownloadTask`, associates it with `episode_id`, and starts
    /// emitting `Progress` reports.
    ///
    /// `expected_bytes` is an optional hint (e.g. from the RSS enclosure's
    /// `length` attribute); the executor will report the authoritative
    /// `total_bytes` once the server replies with `Content-Length`.
    StartDownload {
        /// HTTP/HTTPS URL of the enclosure.
        url: String,
        /// Stable episode id the capability uses to correlate progress.
        /// Mirrors `taskDescription` on `URLSessionDownloadTask`.
        episode_id: String,
        /// Optional pre-flight size hint; `None` if the feed didn't provide
        /// `enclosure.length`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_bytes: Option<u64>,
    },
    /// Pause the active download for `episode_id`. The executor calls
    /// `URLSessionDownloadTask.cancel(byProducingResumeData:)` and emits a
    /// `Paused` report carrying the bytes-so-far.
    PauseDownload { episode_id: String },
    /// Resume the previously paused download for `episode_id`. The executor
    /// rehydrates the resume data (stored on the iOS side keyed by
    /// `episode_id`) and starts a new task from the saved offset.
    ResumeDownload { episode_id: String },
    /// Cancel the active or queued download for `episode_id` outright. The
    /// executor calls `URLSessionDownloadTask.cancel()` (no resume data) and
    /// emits a `Cancelled` report.
    CancelDownload { episode_id: String },
    /// Cancel every download the executor is currently running. Used on app
    /// shutdown / sign-out. The executor emits one `Cancelled` report per
    /// active download.
    CancelAll,
}

impl DownloadCommand {
    /// Convenience: construct a `StartDownload` command from owned strings.
    #[must_use]
    pub fn start(
        url: impl Into<String>,
        episode_id: impl Into<String>,
        expected_bytes: Option<u64>,
    ) -> Self {
        Self::StartDownload {
            url: url.into(),
            episode_id: episode_id.into(),
            expected_bytes,
        }
    }

    /// Convenience: construct a `CancelDownload` command.
    #[must_use]
    pub fn cancel(episode_id: impl Into<String>) -> Self {
        Self::CancelDownload {
            episode_id: episode_id.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// iOS → Rust: DownloadReport
// ---------------------------------------------------------------------------

/// Events the iOS download capability sends back to Rust.
///
/// Wire form is `serde`-tagged on `"type"` (`snake_case`).
///
/// **D7:** these are *observations* of what the background `URLSession`
/// actually did, not invitations for Rust to decide something. The iOS side
/// never includes a "you should do X" field; the kernel projects the report
/// into [`crate::download::DownloadQueue`] state and emits any follow-up
/// [`DownloadCommand`] from its own state machine (e.g. starting the next
/// queued item when a slot frees up).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadReport {
    /// Incremental progress for an in-flight download. The executor
    /// throttles these to ≤1 Hz per the canonical §5.2 budget so the
    /// kernel doesn't churn re-rendering the UI mid-fetch.
    ///
    /// `total_bytes` is `None` until the server reports `Content-Length`.
    Progress {
        episode_id: String,
        bytes_downloaded: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_bytes: Option<u64>,
    },
    /// The download finished successfully. `local_path` is the file the
    /// executor moved the temporary download to (legacy: `Application
    /// Support/Downloads/<episode_id>.<ext>`). The kernel projects this
    /// into `DownloadItemState::Completed` and may immediately emit a
    /// `StartDownload` for the next queued item.
    Completed {
        episode_id: String,
        local_path: String,
    },
    /// The download failed. `error` is a human-readable diagnostic
    /// (NSError `localizedDescription` or HTTP status). The kernel
    /// projects this into `DownloadItemState::Failed`; retry policy
    /// (whether to re-enqueue with backoff) lives in Rust, not here.
    Failed { episode_id: String, error: String },
    /// The download was cancelled — either by an explicit `CancelDownload`
    /// command, by `CancelAll`, or by an external interruption the
    /// executor surfaces as a cancel. Frees a concurrency slot.
    Cancelled { episode_id: String },
    /// The download was paused. `bytes_downloaded` is the offset the
    /// executor stashed alongside resume data; resume tokens themselves
    /// live in the iOS-side keyed store so the next `ResumeDownload`
    /// command can pick them up without a Rust round-trip.
    Paused {
        episode_id: String,
        bytes_downloaded: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_command_start_serde_roundtrips() {
        let cmd = DownloadCommand::start("https://ex.com/ep.mp3", "ep-7", Some(12345));
        let json = serde_json::to_string(&cmd).expect("encode");
        assert!(json.contains("\"type\":\"start_download\""));
        assert!(json.contains("\"url\":\"https://ex.com/ep.mp3\""));
        assert!(json.contains("\"episode_id\":\"ep-7\""));
        assert!(json.contains("\"expected_bytes\":12345"));
        let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn download_command_start_omits_none_expected_bytes() {
        let cmd = DownloadCommand::start("https://ex.com/ep.mp3", "ep-7", None);
        let json = serde_json::to_string(&cmd).expect("encode");
        // `skip_serializing_if = "Option::is_none"` keeps the wire payload tidy.
        assert!(!json.contains("expected_bytes"));
        let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn download_command_cancel_all_has_no_payload() {
        assert_eq!(
            serde_json::to_string(&DownloadCommand::CancelAll).expect("encode"),
            r#"{"type":"cancel_all"}"#
        );
    }

    #[test]
    fn download_command_pause_resume_cancel_round_trip() {
        for cmd in [
            DownloadCommand::PauseDownload {
                episode_id: "ep-1".into(),
            },
            DownloadCommand::ResumeDownload {
                episode_id: "ep-1".into(),
            },
            DownloadCommand::CancelDownload {
                episode_id: "ep-1".into(),
            },
        ] {
            let json = serde_json::to_string(&cmd).expect("encode");
            let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
            assert_eq!(decoded, cmd);
        }
    }

    #[test]
    fn download_report_progress_serde_roundtrips() {
        let rep = DownloadReport::Progress {
            episode_id: "ep-1".into(),
            bytes_downloaded: 4096,
            total_bytes: Some(81920),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn download_report_progress_total_bytes_optional() {
        let rep = DownloadReport::Progress {
            episode_id: "ep-1".into(),
            bytes_downloaded: 4096,
            total_bytes: None,
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(!json.contains("total_bytes"));
        let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn download_report_completed_carries_local_path() {
        let rep = DownloadReport::Completed {
            episode_id: "ep-1".into(),
            local_path: "/var/mobile/.../ep-1.mp3".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("ep-1.mp3"));
        let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn download_report_failed_carries_error() {
        let rep = DownloadReport::Failed {
            episode_id: "ep-1".into(),
            error: "transport: timeout".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(json.contains("\"type\":\"failed\""));
        assert!(json.contains("transport: timeout"));
    }

    #[test]
    fn download_report_paused_carries_bytes() {
        let rep = DownloadReport::Paused {
            episode_id: "ep-1".into(),
            bytes_downloaded: 2048,
        };
        let json = serde_json::to_string(&rep).expect("encode");
        let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn namespace_matches_canonical_capability_plan() {
        assert_eq!(DOWNLOAD_CAPABILITY_NAMESPACE, "nmp.download.capability");
    }
}

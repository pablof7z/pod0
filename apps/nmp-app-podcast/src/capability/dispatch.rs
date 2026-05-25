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

use crate::capability::{AudioCommand, AudioReport, DownloadReport};
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
/// The kernel projects the report into [`PodcastStore::local_paths`]
/// (and, in a follow-up, into `crate::download::DownloadQueue`); any
/// resulting follow-up [`crate::capability::DownloadCommand`] (e.g.
/// "start the next queued item") will be driven by the queue state
/// machine, not synthesised here.
///
/// Today the projection is narrowly scoped:
///   * `Completed { local_path }` — records the on-disk path so
///     [`crate::ffi::EpisodeSummary::download_path`] becomes non-null
///     on the next snapshot.
///   * Every other variant (`Progress`, `Failed`, `Cancelled`, `Paused`)
///     decodes cleanly and resolves to `DispatchOutcome::Ok` with no
///     store mutation — the richer queue projection lands in a later
///     PR alongside `DownloadQueueSnapshot` writes.
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
    apply_download_report(store, report);
    DispatchOutcome::Ok { follow_up_json: None }
}

/// Pure projection of a typed [`DownloadReport`] onto `store`. Split out
/// so unit tests don't have to round-trip through JSON.
fn apply_download_report(store: &mut PodcastStore, report: DownloadReport) {
    match report {
        DownloadReport::Completed { episode_id, local_path } => {
            if let Some((typed_id, _url)) = store.episode_enclosure_url(&episode_id) {
                store.set_local_path(typed_id, local_path);
            }
            // Episode not in the store (e.g. unsubscribed mid-flight):
            // drop the report on the floor. D6 — data, not exception.
        }
        DownloadReport::Cancelled { episode_id } => {
            if let Some((typed_id, _url)) = store.episode_enclosure_url(&episode_id) {
                let _ = store.clear_local_path(&typed_id);
            }
        }
        DownloadReport::Failed { .. }
        | DownloadReport::Paused { .. }
        | DownloadReport::Progress { .. } => {
            // M4.C scope: queue projection lands in a follow-up PR.
            // We decode cleanly so future schema additions (new variants)
            // don't drop reports silently before the queue is wired.
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn t0() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    #[test]
    fn playing_report_json_round_trip_no_follow_up() {
        let mut actor = PlayerActor::new();
        let report = r#"{"type":"playing","url":"u","position_secs":1.0,"duration_secs":10.0}"#;
        let outcome = dispatch_audio_report_json(&mut actor, report, t0());
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => assert!(follow_up_json.is_none()),
            DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
        }
        assert!(actor.state().is_playing);
        assert_eq!(actor.state().position_secs, 1.0);
    }

    #[test]
    fn sleep_timer_fired_emits_stop_command_json() {
        let mut actor = PlayerActor::new();
        actor.arm_sleep_timer(Duration::from_secs(60), t0());
        let outcome =
            dispatch_audio_report_json(&mut actor, r#"{"type":"sleep_timer_fired"}"#, t0());
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => {
                assert_eq!(follow_up_json.as_deref(), Some(r#"{"type":"stop"}"#));
            }
            DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
        }
    }

    #[test]
    fn malformed_report_returns_decode_failed() {
        let mut actor = PlayerActor::new();
        let outcome = dispatch_audio_report_json(&mut actor, "not-json", t0());
        assert!(matches!(outcome, DispatchOutcome::DecodeFailed { .. }));
        // Actor state untouched on a decode failure.
        assert!(!actor.state().is_playing);
    }

    #[test]
    fn encode_audio_command_round_trips() {
        let cmd = AudioCommand::seek(99.0);
        let json = encode_audio_command(&cmd).expect("encode");
        let decoded: AudioCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }

    // ── DownloadReport dispatch ─────────────────────────────────────────────

    use chrono::Utc;
    use podcast_core::{Episode, Podcast};
    use url::Url;

    /// Build a store with one podcast + one episode and return the episode id
    /// as the string the wire format uses.
    fn store_with_one_episode() -> (PodcastStore, String) {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Show");
        let ep = Episode::new(
            podcast.id,
            "guid-1",
            "Title",
            Url::parse("https://ex.com/ep.mp3").expect("url"),
            Utc::now(),
        );
        let id_str = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);
        (store, id_str)
    }

    #[test]
    fn completed_report_records_local_path() {
        let (mut store, id_str) = store_with_one_episode();
        let report = format!(
            r#"{{"type":"completed","episode_id":"{id_str}","local_path":"/var/mobile/Downloads/{id_str}.mp3"}}"#
        );
        let outcome = dispatch_download_report_json(&mut store, &report);
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => assert!(follow_up_json.is_none()),
            DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
        }
        let typed_id = store
            .episode_enclosure_url(&id_str)
            .map(|(id, _)| id)
            .expect("episode present");
        assert_eq!(
            store.local_path_for(&typed_id),
            Some(&*format!("/var/mobile/Downloads/{id_str}.mp3"))
        );
    }

    #[test]
    fn completed_report_for_unknown_episode_is_noop() {
        let (mut store, _) = store_with_one_episode();
        // A `completed` for an episode the store has never seen (e.g. user
        // unsubscribed mid-download) drops on the floor — D6.
        let report =
            r#"{"type":"completed","episode_id":"00000000-0000-0000-0000-000000000000","local_path":"/tmp/x.mp3"}"#;
        let outcome = dispatch_download_report_json(&mut store, report);
        assert!(matches!(outcome, DispatchOutcome::Ok { .. }));
    }

    #[test]
    fn cancelled_report_clears_local_path() {
        let (mut store, id_str) = store_with_one_episode();
        // Seed a local path so we can observe the clear.
        let typed_id = store
            .episode_enclosure_url(&id_str)
            .map(|(id, _)| id)
            .expect("episode present");
        store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into());
        assert!(store.local_path_for(&typed_id).is_some());

        let report = format!(r#"{{"type":"cancelled","episode_id":"{id_str}"}}"#);
        let outcome = dispatch_download_report_json(&mut store, &report);
        assert!(matches!(outcome, DispatchOutcome::Ok { .. }));
        assert!(store.local_path_for(&typed_id).is_none());
    }

    #[test]
    fn progress_failed_paused_decode_without_mutating_store() {
        let (mut store, id_str) = store_with_one_episode();
        let typed_id = store
            .episode_enclosure_url(&id_str)
            .map(|(id, _)| id)
            .expect("episode present");
        // Pre-seed a path so we can confirm nothing wipes it.
        store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into());

        for report in [
            format!(
                r#"{{"type":"progress","episode_id":"{id_str}","bytes_downloaded":4096,"total_bytes":81920}}"#
            ),
            format!(r#"{{"type":"failed","episode_id":"{id_str}","error":"timeout"}}"#),
            format!(r#"{{"type":"paused","episode_id":"{id_str}","bytes_downloaded":1024}}"#),
        ] {
            let outcome = dispatch_download_report_json(&mut store, &report);
            assert!(
                matches!(outcome, DispatchOutcome::Ok { .. }),
                "{report} should decode cleanly"
            );
        }
        // None of the non-completed reports touched the local path.
        assert_eq!(store.local_path_for(&typed_id), Some("/var/mobile/seeded.mp3"));
    }

    #[test]
    fn malformed_download_report_returns_decode_failed() {
        let (mut store, _) = store_with_one_episode();
        let outcome = dispatch_download_report_json(&mut store, "not-json");
        assert!(matches!(outcome, DispatchOutcome::DecodeFailed { .. }));
    }
}

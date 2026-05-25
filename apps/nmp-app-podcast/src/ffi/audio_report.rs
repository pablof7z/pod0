//! `nmp_app_podcast_audio_report` — async iOS→Rust audio-report channel.
//!
//! The iOS `AudioCapability` fires this FFI entry point whenever it has a new
//! `AudioReport` to deliver (time ticks, track-end, sleep-timer-fired, …).
//! Rust applies the report to the `PlayerActor` state machine and returns any
//! follow-up `AudioCommand` the iOS side should immediately execute.
//!
//! ## Wire protocol
//!
//! * **Request**: `report_json` is a JSON-encoded [`crate::capability::AudioReport`].
//! * **Response**: heap-allocated nul-terminated JSON of an
//!   [`crate::capability::AudioCommand`], or `NULL` when no follow-up is needed.
//!   The caller MUST free the returned pointer via `nmp_app_free_string`.
//!
//! ## Position writeback (feature #12)
//!
//! After the report has been dispatched into the actor we mirror the live
//! playhead into the matching `Episode.position_secs` on the `PodcastStore`
//! so the resume point survives a process restart. `Playing` ticks arrive at
//! ≤4 Hz (`AudioReport` D8) so the mutation stays in-memory; we only flush to
//! disk on terminal events (`Paused` / `Stopped` / `SleepTimerFired`) and on
//! a coarse position-delta threshold so a long unbroken playback session
//! still checkpoints every ~30 seconds of playhead.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and decode failures all return
//! `NULL` (treated by iOS as "no follow-up command"). Nothing panics.

use std::ffi::{c_char, CStr, CString};
use std::time::SystemTime;

use super::handle::PodcastHandle;
use crate::capability::AudioReport;
use crate::store::PodcastStore;

/// Minimum position delta (seconds) between disk flushes while a `Playing`
/// stream is in flight. Keeps the on-disk checkpoint within ~30 s of the live
/// playhead without burning a write on every `Playing` tick (≤4 Hz).
const POSITION_FLUSH_DELTA_SECS: f64 = 30.0;

/// Deliver a JSON-encoded `AudioReport` to the Rust `PlayerActor` and return
/// the JSON-encoded follow-up `AudioCommand`, if any.
///
/// Returns a malloc-compatible string the caller MUST free via `nmp_app_free_string`,
/// or `NULL` when no follow-up is needed (or on any error).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_audio_report(
    handle: *mut PodcastHandle,
    report_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || report_json.is_null() {
        return std::ptr::null_mut();
    }

    let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle_ref = unsafe { &*handle };

    // Decode once so we can both (a) project into the actor and
    // (b) decide whether the report carries a position to mirror into
    // the store. Dispatching a second time would re-pay the JSON cost
    // and risk the actor and the store seeing different decodes.
    let report: AudioReport = match serde_json::from_str(report_str) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    // -- 1. Project the report into the actor under its own lock. -----
    let (follow_up_json, episode_id_for_writeback) = {
        let mut actor = match handle_ref.player_actor.lock() {
            Ok(a) => a,
            Err(_) => return std::ptr::null_mut(),
        };
        let follow_up = actor.handle_audio_report(report.clone(), SystemTime::now());
        let follow_up_json = follow_up.and_then(|cmd| serde_json::to_string(&cmd).ok());
        // The episode id stays in actor state across `Playing` / `Paused`
        // ticks (it's only cleared on `Stopped`); read it here so the
        // writeback step doesn't need to crack the report again.
        let episode_id = actor.state().episode_id.clone();
        drop(actor); // release before rev bump and store lock
        handle_ref
            .rev
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        (follow_up_json, episode_id)
    };

    // -- 2. Mirror the playhead into the store. -----------------------
    if let Some(episode_id) = episode_id_for_writeback {
        if let Ok(mut store) = handle_ref.store.lock() {
            apply_writeback(&mut store, &report, &episode_id);
        }
    }

    match follow_up_json {
        Some(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// Mirror the playhead from `report` into `Episode.position_secs` for
/// `episode_id`. Flushes to disk on terminal events (Paused / Stopped /
/// SleepTimerFired) and on a coarse position-delta threshold during
/// `Playing` so a long uninterrupted stream still checkpoints.
///
/// The throttling threshold compares against the most-recent **flushed**
/// position (`store.last_flushed_position`), not the previous tick's
/// in-memory value. Comparing against the previous tick would never
/// trigger during a real ≤4 Hz playback stream (each tick advances ~0.25 s
/// and the diff stays tiny forever).
fn apply_writeback(store: &mut PodcastStore, report: &AudioReport, episode_id: &str) {
    match report {
        AudioReport::Playing { position_secs, .. } => {
            let last_flushed = store.last_flushed_position(episode_id).unwrap_or(0.0);
            if !store.set_episode_position(episode_id, *position_secs) {
                return; // no matching episode — nothing to flush
            }
            if (*position_secs - last_flushed).abs() >= POSITION_FLUSH_DELTA_SECS {
                store.flush_positions();
            }
        }
        AudioReport::Paused { position_secs, .. } => {
            if store.set_episode_position(episode_id, *position_secs) {
                store.flush_positions();
            }
        }
        AudioReport::Stopped | AudioReport::SleepTimerFired => {
            // No fresh position in the payload — the most-recent `Playing`
            // tick already updated in-memory state. Flush so the checkpoint
            // survives a hard kill.
            store.flush_positions();
        }
        AudioReport::Failed { .. } | AudioReport::BufferingProgress { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, PodcastId};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use url::Url;
    use uuid::Uuid;

    /// RAII tempdir local to this module so the writeback tests are
    /// self-contained and don't pull in `tempfile`.
    struct TempDir {
        path: PathBuf,
    }
    impl TempDir {
        fn new(label: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nmp-audio-report-{}-{}-{}",
                label,
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
        Episode::new(
            podcast_id,
            format!("guid-{}", Uuid::new_v4()),
            title,
            Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        )
    }

    #[test]
    fn playing_report_writes_position_back_to_store() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Resume Show");
        let pid = podcast.id;
        let ep = make_episode(pid, "Ep");
        let ep_id = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        let report = AudioReport::Playing {
            url: "https://example.com/audio.mp3".into(),
            position_secs: 17.0,
            duration_secs: 1800.0,
        };
        apply_writeback(&mut store, &report, &ep_id);

        assert_eq!(store.position_for(&ep_id), Some(17.0));
    }

    #[test]
    fn paused_report_flushes_to_disk() {
        let dir = TempDir::new("paused");
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = Podcast::new("Pause Flush");
        let pid = podcast.id;
        let ep = make_episode(pid, "Ep");
        let ep_id = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        let report = AudioReport::Paused {
            url: "https://example.com/audio.mp3".into(),
            position_secs: 42.0,
        };
        apply_writeback(&mut store, &report, &ep_id);

        let mut reloaded = PodcastStore::new();
        reloaded.set_data_dir(dir.path.clone());
        assert_eq!(reloaded.position_for(&ep_id), Some(42.0));
    }

    #[test]
    fn playing_ticks_only_flush_after_position_delta() {
        let dir = TempDir::new("throttle");
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = Podcast::new("Throttle");
        let pid = podcast.id;
        let ep = make_episode(pid, "Ep");
        let ep_id = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        // Two close ticks — neither crosses the delta, so the on-disk file
        // should still report position 0 after reload.
        apply_writeback(
            &mut store,
            &AudioReport::Playing {
                url: "u".into(),
                position_secs: 5.0,
                duration_secs: 600.0,
            },
            &ep_id,
        );
        apply_writeback(
            &mut store,
            &AudioReport::Playing {
                url: "u".into(),
                position_secs: 10.0,
                duration_secs: 600.0,
            },
            &ep_id,
        );
        let mut reloaded_before = PodcastStore::new();
        reloaded_before.set_data_dir(dir.path.clone());
        assert_eq!(reloaded_before.position_for(&ep_id), None);

        // A tick that crosses the 30 s delta triggers a flush.
        apply_writeback(
            &mut store,
            &AudioReport::Playing {
                url: "u".into(),
                position_secs: 45.0,
                duration_secs: 600.0,
            },
            &ep_id,
        );
        let mut reloaded_after = PodcastStore::new();
        reloaded_after.set_data_dir(dir.path.clone());
        assert_eq!(reloaded_after.position_for(&ep_id), Some(45.0));
    }

    #[test]
    fn unknown_episode_id_is_a_noop() {
        let mut store = PodcastStore::new();
        // Empty store → `set_episode_position` returns false → no flush,
        // no panic, no disk-touch attempt.
        apply_writeback(
            &mut store,
            &AudioReport::Playing {
                url: "u".into(),
                position_secs: 1.0,
                duration_secs: 60.0,
            },
            "no-such-episode",
        );
    }

    #[test]
    fn failed_and_buffering_reports_do_not_mutate_position() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Inert Reports");
        let pid = podcast.id;
        let mut ep = make_episode(pid, "Ep");
        ep.position_secs = 12.0;
        let ep_id = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        apply_writeback(
            &mut store,
            &AudioReport::BufferingProgress { fraction: 0.5 },
            &ep_id,
        );
        apply_writeback(
            &mut store,
            &AudioReport::Failed {
                url: "u".into(),
                error: "boom".into(),
            },
            &ep_id,
        );
        assert_eq!(store.position_for(&ep_id), Some(12.0));
    }

    /// Regression for the throttling bug: 200 small ≤4 Hz ticks (typical of a
    /// real playback stream, each advancing ~0.25 s) must still produce at
    /// least one mid-stream flush so a hard kill loses at most one delta of
    /// position. The earlier `prev = position_for(...)` comparison made this
    /// loop never flush — the fix anchors the throttle to the last
    /// **flushed** position instead.
    #[test]
    fn continuous_playback_checkpoints_periodically() {
        let dir = TempDir::new("continuous");
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = Podcast::new("Continuous");
        let pid = podcast.id;
        let ep = make_episode(pid, "Ep");
        let ep_id = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        // 200 ticks at 0.25 s each = 50 s of playback. At a 30 s flush
        // threshold the stream should checkpoint at least once mid-stream.
        for i in 1..=200 {
            apply_writeback(
                &mut store,
                &AudioReport::Playing {
                    url: "u".into(),
                    position_secs: (i as f64) * 0.25,
                    duration_secs: 3600.0,
                },
                &ep_id,
            );
        }

        // Reload from disk without flushing — the on-disk position must be
        // past the first 30 s threshold (so a kill mid-stream loses at most
        // ~30 s, not the entire 50 s).
        let mut reloaded = PodcastStore::new();
        reloaded.set_data_dir(dir.path.clone());
        let on_disk = reloaded.position_for(&ep_id).expect("checkpointed");
        assert!(
            on_disk >= 30.0,
            "expected an on-disk checkpoint past 30 s, got {on_disk}"
        );
    }
}

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

use nmp_core::substrate::CapabilityRequest;

use super::handle::PodcastHandle;
use crate::capability::{
    AudioCommand, AudioReport, DownloadCommand, AUDIO_CAPABILITY_NAMESPACE,
    DOWNLOAD_CAPABILITY_NAMESPACE,
};
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
    let is_item_end = matches!(report, AudioReport::ItemEnd { .. });
    if let Some(ref episode_id) = episode_id_for_writeback {
        if let Ok(mut store) = handle_ref.store.lock() {
            apply_writeback(&mut store, &report, episode_id);
        }
    }

    // -- 3. M1.3: Auto-advance on natural end. -------------------------
    // When `ItemEnd` fires and `auto_play_next` is armed, pop the next
    // queued episode and dispatch Load + Play directly — the actor does
    // not do this internally because it has no access to the store (URLs)
    // or the capability dispatcher.
    if is_item_end {
        maybe_auto_advance(handle_ref);
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
        AudioReport::ItemEnd { .. } => {
            // Natural play-to-completion. Gate the "mark listened" write on
            // the store's `auto_mark_played_at_end` flag (M1.3). The
            // position is already up-to-date from the last `Playing` tick.
            if store.auto_mark_played_at_end() {
                store.mark_episode_played(episode_id);
            }
            store.flush_positions();
        }
        AudioReport::Failed { .. } | AudioReport::BufferingProgress { .. } => {}
    }
}

/// M1.3 — auto-advance on natural end.
///
/// Called only on `ItemEnd`. Reads the actor's `auto_play_next` flag and
/// queue. If conditions are met, pops the next episode, stages a load in
/// the actor, and dispatches `Load` + `Play` back to iOS via the
/// capability channel. Does nothing (silently) on any lock failure.
fn maybe_auto_advance(handle: &PodcastHandle) {
    // Read the flag + pop atomically under the actor lock.
    let next_episode_id = {
        let mut actor = match handle.player_actor.lock() {
            Ok(a) => a,
            Err(_) => return,
        };
        if !actor.auto_play_next || actor.queue().is_empty() {
            return;
        }
        actor.pop_next()
    };

    let Some(episode_id) = next_episode_id else { return; };

    // Look up playback info for the next episode.
    let (podcast_id, url, position_secs) = match handle.store.lock() {
        Ok(s) => match s.episode_playback_info(&episode_id) {
            Some(info) => info,
            None => return, // episode disappeared from library
        },
        Err(_) => return,
    };

    // Stage the new load on the actor.
    if let Ok(mut actor) = handle.player_actor.lock() {
        actor.stage_load(&episode_id, Some(podcast_id), &url, position_secs);
    }

    // Dispatch Load + Play. Failures degrade silently per D6.
    dispatch_audio_cmd(handle, &AudioCommand::load(&url, position_secs));
    dispatch_audio_cmd(handle, &AudioCommand::Play);

    // Enqueue a background download for un-downloaded episodes (mirrors
    // handle_play). `DownloadQueue::enqueue` is idempotent.
    let needs_dl = match handle.store.lock() {
        Ok(s) => !s.episode_is_downloaded(&episode_id),
        Err(_) => false,
    };
    if needs_dl {
        if let Ok(mut q) = handle.download_queue.lock() {
            if let Some(cmd) = q.enqueue(episode_id.clone(), url.clone()) {
                dispatch_download_cmd(handle, &cmd);
            }
        }
    }

    handle.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn dispatch_audio_cmd(handle: &PodcastHandle, cmd: &AudioCommand) {
    let payload_json = match serde_json::to_string(cmd) {
        Ok(j) => j,
        Err(_) => return,
    };
    let req = CapabilityRequest {
        namespace: AUDIO_CAPABILITY_NAMESPACE.to_owned(),
        correlation_id: String::new(),
        payload_json,
    };
    let _ = unsafe { &*handle.app }.dispatch_capability(&req);
}

fn dispatch_download_cmd(handle: &PodcastHandle, cmd: &DownloadCommand) {
    let payload_json = match serde_json::to_string(cmd) {
        Ok(j) => j,
        Err(_) => return,
    };
    let req = CapabilityRequest {
        namespace: DOWNLOAD_CAPABILITY_NAMESPACE.to_owned(),
        correlation_id: String::new(),
        payload_json,
    };
    let _ = unsafe { &*handle.app }.dispatch_capability(&req);
}


// Tests split into audio_report_tests.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "audio_report_tests.rs"]
mod tests;

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
//!   The caller MUST free the returned pointer via `nmp_free_string`.
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
use serde::Serialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::capability::{
    AudioCommand, AudioReport, DownloadCommand, AUDIO_CAPABILITY_NAMESPACE,
    DOWNLOAD_CAPABILITY_NAMESPACE,
};
use crate::player::PlayerState;
use crate::store::PodcastStore;

/// JSON response shape returned to the Swift audio-report channel. Mirrors
/// `DownloadReportResponse` (see `download_report.rs`): live state rides the
/// inline payload, and only structural changes bump the global `rev`.
///
/// Fields are decoded on the Swift side with `convertFromSnakeCase`.
#[derive(Serialize)]
struct AudioReportResponse {
    /// JSON of the follow-up `AudioCommand` (e.g. the `Stop` a sleep-timer
    /// fires), omitted when there's nothing to execute. Carried as a *string*
    /// (not a nested object) so Swift decodes it with a plain decoder —
    /// `AudioCommand` uses coding keys a `convertFromSnakeCase` pass would break.
    #[serde(skip_serializing_if = "Option::is_none")]
    follow_up: Option<String>,
    /// Fresh player state so Swift updates its live `nowPlaying` (scrubber,
    /// Dynamic Island, lock screen) without pulling the full library. `None`
    /// when nothing is loaded. Same shape as `PodcastUpdate.now_playing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    now_playing: Option<PlayerState>,
    /// `true` when the report changed structural state (play/pause/stop, track
    /// end, sleep-timer). Swift pulls the full snapshot only when this is set;
    /// `Playing`/`BufferingProgress` ticks leave it `false` and ride the inline
    /// `now_playing` instead — that is the ~1 Hz hot path this split removes
    /// from the full-rebuild routine.
    durable_changed: bool,
}

/// Minimum position delta (seconds) between disk flushes while a `Playing`
/// stream is in flight. Keeps the on-disk checkpoint within ~30 s of the live
/// playhead without burning a write on every `Playing` tick (≤4 Hz).
const POSITION_FLUSH_DELTA_SECS: f64 = 30.0;

/// Deliver a JSON-encoded `AudioReport` to the Rust `PlayerActor` and return
/// the JSON-encoded follow-up `AudioCommand`, if any.
///
/// Returns a malloc-compatible string the caller MUST free via `nmp_free_string`,
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
    ffi_guard("nmp_app_podcast_audio_report", std::ptr::null_mut, || {
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

        // `Playing` (≤4 Hz position) and `BufferingProgress` ticks carry only
        // live player state — the fresh `now_playing` rides the response inline,
        // so they must NOT bump the global `rev`. Bumping it on every tick
        // invalidated the snapshot cache and forced a full-library rebuild +
        // main-thread JSON decode of the whole library ~1 Hz throughout playback
        // (the residual CPU peg after the download path was split the same way).
        // Every other report is structural (play/pause/stop, track end,
        // sleep-timer) and bumps `rev` so the full library projection re-runs.
        let durable_changed = !matches!(
            report,
            AudioReport::Playing { .. } | AudioReport::BufferingProgress { .. }
        );

        // -- 1. Project the report into the actor; capture fresh now_playing. --
        // Step 14: player_actor sourced from state.playback.player via .share().
        // Lock topology UNCHANGED — same Arc<Mutex<PlayerActor>>, sourced differently.
        let (follow_up_json, now_playing, episode_id_for_writeback) = {
            let mut actor = match handle_ref.state.playback.player.lock() {
                Ok(a) => a,
                Err(_) => return std::ptr::null_mut(),
            };
            let follow_up = actor.handle_audio_report(report.clone(), SystemTime::now());
            let follow_up_json = follow_up.and_then(|cmd| serde_json::to_string(&cmd).ok());
            let state = actor.state();
            // The episode id stays in actor state across `Playing` / `Paused`
            // ticks (it's only cleared on `Stopped`); read it here so the
            // writeback step doesn't need to crack the report again.
            let episode_id = state.episode_id.clone();
            // Mirror the full snapshot's `now_playing` projection: present only
            // when an episode is loaded (`build_podcast_update`).
            let now_playing = if state.episode_id.is_some() {
                Some(state.clone())
            } else {
                None
            };
            drop(actor); // release before rev bump and store lock
            // A durable audio report (mark-played-at-end, sleep-timer stop)
            // changes the episode's played/position state — both live in the
            // `podcast.library` payload — so route the delta there.
            handle_ref.bump_snapshot_rev_domain_if(crate::state::Domain::Library, durable_changed);
            (follow_up_json, now_playing, episode_id)
        };

        // -- 2. Mirror the playhead into the store. ----------------------------
        let is_item_end = matches!(report, AudioReport::ItemEnd { .. });
        if let Some(ref episode_id) = episode_id_for_writeback {
            if let Ok(mut store) = handle_ref.state.library.store.lock() {
                apply_writeback(&mut store, &report, episode_id);
            }
        }

        // -- 3. M1.3: Auto-advance on natural end. -----------------------------
        // When `ItemEnd` fires and `auto_play_next` is armed, pop the next
        // queued episode and dispatch Load + Play directly — the actor does
        // not do this internally because it has no access to the store (URLs)
        // or the capability dispatcher. (`ItemEnd` is durable, so `rev` already
        // bumped above; `maybe_auto_advance` bumps again after staging the load.)
        if is_item_end {
            maybe_auto_advance(handle_ref);
        }

        let response = AudioReportResponse {
            follow_up: follow_up_json,
            now_playing,
            durable_changed,
        };
        match serde_json::to_string(&response) {
            Ok(json) => match CString::new(json) {
                Ok(c) => c.into_raw(),
                Err(_) => std::ptr::null_mut(),
            },
            Err(_) => std::ptr::null_mut(),
        }
    })
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
            // the store's `auto_mark_played_at_end` flag (M1.3).
            if store.auto_mark_played_at_end() {
                store.mark_episode_played(episode_id);
                // Delete-after-played is kernel-owned policy (D0). Now that the
                // episode is marked played, honour the user's
                // `auto_delete_downloads_after_played` setting by dropping the
                // local download and removing the file. Only runs when the
                // mark actually happened (i.e. `auto_mark_played_at_end` is on),
                // matching the prior Swift `onItemEnd` gate.
                if let Some(path) = store.clear_local_path_if_auto_delete(episode_id) {
                    let _ = std::fs::remove_file(&path);
                }
            }
            // Rewind to the start on natural completion so the next play begins
            // from 0 instead of resuming at the end. `mark_episode_played` only
            // flips the played flag, and the engine emits a `Paused` at
            // `duration` just before `ItemEnd`, so without this the stored
            // position is the duration and replay lands at the end. Runs
            // regardless of `auto_mark_played_at_end` — a finished episode should
            // always restart cleanly.
            store.set_episode_position(episode_id, 0.0);
            store.flush_positions();
        }
        AudioReport::Failed { .. } | AudioReport::BufferingProgress { .. } => {}
    }
}

/// M1.3 — auto-advance on natural end.
///
/// Called only on `ItemEnd`. Reads the actor's `auto_play_next` flag, pops the
/// next episode from the canonical playback queue, stages a load in the actor,
/// and dispatches `Load` + `Play` back to iOS via the capability channel. Does
/// nothing (silently) on any lock failure.
fn maybe_auto_advance(handle: &PodcastHandle) {
    // Step 14: player, queue, and download_queue are sourced from
    // state.playback.* via .share() — same Arc<Mutex<_>>, different address.
    // Lock topology UNCHANGED (never nested; guard dropped before next lock).

    // The `auto_play_next` flag lives on the actor (mirrored from settings).
    let auto_play_next = match handle.state.playback.player.lock() {
        Ok(a) => a.auto_play_next,
        Err(_) => return,
    };
    if !auto_play_next {
        return;
    }

    // Pop the next RESOLVABLE episode from the canonical queue, skipping stale
    // heads (episodes removed from library / unsubscribed shows). Queue and
    // store locks are taken separately per iteration (never nested) to avoid
    // lock-order hazards.
    let (episode_id, podcast_id, url, position_secs) = loop {
        let popped = match handle.state.playback.queue.lock() {
            Ok(mut q) => q.next(),
            Err(_) => return,
        };
        let Some(id) = popped else { return }; // queue exhausted — nothing to play
        let info = match handle.state.library.store.lock() {
            Ok(s) => s.episode_playback_info(&id),
            Err(_) => return,
        };
        // Stage the store's canonical (lowercase) id so the actor's
        // `episode_id` stays exact-matchable downstream (see `handle_play`).
        if let Some((canon_id, pod, ep_url, pos)) = info {
            break (canon_id, pod, ep_url, pos);
        }
        // Stale head already popped; continue to the next entry.
    };

    // Stage the new load on the actor and dispatch Load + Play atomically:
    // both must happen together or not at all. If the actor lock is poisoned
    // we cannot record the staged episode (position will never persist, the
    // episode will never be marked played), so we bail entirely rather than
    // starting playback in an unrecorded state. This mirrors the doctrine
    // applied to the lock-screen-play divergence: stage-and-dispatch are one
    // atomic decision; an unrecordable advance is better skipped than started.
    match handle.state.playback.player.lock() {
        Ok(mut actor) => {
            actor.stage_load(&episode_id, Some(podcast_id), &url, position_secs);
        }
        Err(_) => return, // poisoned lock — bail before dispatching Load/Play
    }

    // Dispatch Load + Play. Failures degrade silently per D6.
    dispatch_audio_cmd(
        handle,
        &AudioCommand::load_with_id(&url, position_secs, &episode_id),
    );
    dispatch_audio_cmd(handle, &AudioCommand::Play);

    // Enqueue a background download for un-downloaded episodes (mirrors
    // handle_play). `DownloadQueue::enqueue` is idempotent.
    let needs_dl = match handle.state.library.store.lock() {
        Ok(s) => !s.episode_is_downloaded(&episode_id),
        Err(_) => false,
    };
    if needs_dl {
        let dl_cmd = match handle.state.playback.downloads.lock() {
            Ok(mut q) => q.enqueue(episode_id.clone(), url.clone()),
            Err(_) => None,
        };
        if let Some(cmd) = dl_cmd {
            dispatch_download_cmd(handle, &cmd);
        }
    }

    // Auto-advance staged a new now_playing episode → the `podcast.playback`
    // delta. (The departing episode's mark-played already bumped library above.)
    handle.bump_snapshot_rev_domain(crate::state::Domain::Playback);
}

fn dispatch_audio_cmd(handle: &PodcastHandle, cmd: &AudioCommand) {
    // D6: a null/uninitialized app pointer (unit tests, pre-`nmp_app_start`)
    // degrades to a no-op rather than dereferencing null.
    if handle.app.is_null() {
        return;
    }
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
    // D6: a null/uninitialized app pointer (unit tests, pre-`nmp_app_start`)
    // degrades to a no-op rather than dereferencing null.
    if handle.app.is_null() {
        return;
    }
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

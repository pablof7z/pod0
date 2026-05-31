//! Kernel-side wiring for `podcast_briefings::BriefingScheduler`.
//!
//! The scheduler crate is a pure, clock-free state machine (see its
//! module doc — "No `Utc::now()`", "No I/O"). This module is the thin
//! kernel seam that supplies the wall clock and routes a due slot into
//! the existing `podcast/generate_briefing` action path. It mirrors the
//! inbox proactive-triage trigger (`inbox_handler::maybe_enqueue_triage`):
//! a cheap predicate runs on every snapshot tick off the actor thread,
//! and only fires real work when a slot is genuinely due.
//!
//! ## Local wall clock
//!
//! `BriefingSchedule::time_of_day` is documented as *minutes since
//! midnight, local time* and `days` as *local weekday* (0 = Sunday).
//! We therefore derive the tick instant from [`chrono::Local`] — feeding
//! `Utc::now()` minutes would fire the briefing at the wrong local hour.
//! `created_at` on the minted [`Briefing`] is still stored as `Utc`
//! (the domain type's field type), converted from the same instant.
//!
//! ## Latch / dedup
//!
//! `should_generate_now` returns false the moment `pending.is_some()`,
//! so calling [`BriefingScheduler::start_pending`] in the same tick we
//! dispatch latches the slot closed for the rest of the minute (and,
//! today, for the rest of the kernel lifetime — see the note below).
//! Unlike the action path, this trigger does **not** bump `rev` itself;
//! `handle_generate_briefing` already bumps it synchronously, and the
//! background LLM task bumps it again when segments land.
//!
//! ## Scope (M9.B)
//!
//! This PR wires the scheduler plumbing only. Nothing in
//! `nmp-app-podcast` currently calls [`BriefingScheduler::set_schedule`]
//! (there is no registered briefing `ActionModule` yet — the
//! `podcast.briefing.schedule` action is re-exported but unhandled), so
//! in production the scheduler stays schedule-less and this trigger is
//! inert until the schedule-settings action lands in a follow-up. The
//! wiring is exercised by the tests in `briefing_scheduler_tests.rs`,
//! which inject a schedule via `set_schedule`.
//!
//! Because completion never routes back into the scheduler in this PR
//! (the LLM task writes the existing `briefing` snapshot slot, not the
//! scheduler's `pending`), a fired slot is never cleared and the trigger
//! fires at most once per kernel lifetime. Rotating the latch on
//! completion (so a daily schedule fires daily) is tracked as a
//! follow-up alongside the structured script composer.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use chrono::{Datelike, Local, Timelike, Utc};
use podcast_briefings::BriefingScheduler;
use tokio::runtime::Runtime;

use crate::briefings_handler::handle_generate_briefing;
use crate::ffi::projections::BriefingSnapshot;
use crate::store::PodcastStore;

/// Local wall-clock decomposition the scheduler predicate consumes:
/// `(minutes_since_midnight, day_of_week)` where `day_of_week` is
/// `0 = Sunday … 6 = Saturday` (matches `BriefingSchedule::days`).
fn local_now_minutes_dow() -> (u32, u8) {
    let now = Local::now();
    let minutes = now.hour() * 60 + now.minute();
    let dow = now.weekday().num_days_from_sunday() as u8;
    (minutes, dow)
}

/// Snapshot-tick hook: if the configured schedule makes a briefing due
/// *right now* (local time) and none is already pending, mint the
/// pending slot and dispatch the existing `generate_briefing` path.
///
/// Cheap no-op when no schedule is set, when the current minute isn't a
/// scheduled slot, or when a briefing is already pending (the latch).
/// Never blocks the caller — the actual LLM composition happens on the
/// shared runtime inside `handle_generate_briefing`.
///
/// Does **not** bump `rev`; `handle_generate_briefing` owns that.
pub fn maybe_trigger_briefing(
    scheduler: &Arc<Mutex<BriefingScheduler>>,
    briefing_slot: &Arc<Mutex<Option<BriefingSnapshot>>>,
    rev: &Arc<AtomicU64>,
    store: &Arc<Mutex<PodcastStore>>,
    runtime: &Arc<Runtime>,
) {
    let (minutes, dow) = local_now_minutes_dow();

    // Decide + latch under one short lock, then release before dispatch
    // so the LLM round-trip never blocks the scheduler mutex.
    let due = {
        let Ok(mut sched) = scheduler.lock() else {
            return;
        };
        if !sched.should_generate_now(minutes, dow) {
            return;
        }
        // Mint the pending slot in the same tick we decide to fire so the
        // next tick's `should_generate_now` sees `pending.is_some()` and
        // stays quiet (D9 — caller owns the clock).
        let created_at = Local::now().with_timezone(&Utc);
        sched.start_pending(created_at);
        true
    };

    if due {
        // Reuse the existing working action path. It writes the
        // `generating` snapshot slot, bumps `rev`, and spawns the LLM
        // composition on the shared runtime.
        let _ = handle_generate_briefing(briefing_slot, rev, Some(store), Some(runtime));
    }
}

/// Project the scheduler's "minutes until the next scheduled briefing
/// today" onto the briefing snapshot the UI renders.
///
/// Returns `None` when no schedule is active, when today isn't covered,
/// or when today's slot has already passed — matching
/// [`BriefingScheduler::next_scheduled_minutes`]. The snapshot builder
/// folds this onto [`BriefingSnapshot::next_scheduled_minutes`] so iOS
/// can show "next briefing in X minutes" even before the first briefing
/// is composed.
pub fn next_scheduled_minutes(scheduler: &Arc<Mutex<BriefingScheduler>>) -> Option<u32> {
    let (minutes, dow) = local_now_minutes_dow();
    scheduler.lock().ok()?.next_scheduled_minutes(minutes, dow)
}

#[cfg(test)]
#[path = "briefing_scheduler_tests.rs"]
mod tests;

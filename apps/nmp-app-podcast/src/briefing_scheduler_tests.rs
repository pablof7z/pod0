//! Tests for the kernel-side briefing-scheduler trigger.
//!
//! The trigger reads the *real* local wall clock (`Local::now()`), so
//! the tests compute the current local minute/weekday at runtime and
//! build a schedule that covers (or deliberately misses) it. This keeps
//! the assertions deterministic no matter when the suite runs.

use super::*;

use chrono::{Datelike, Local, Timelike};
use podcast_briefings::{BriefingSchedule, BriefingScheduler};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi::projections::BriefingSnapshot;
use crate::store::PodcastStore;

/// Shared Tokio runtime for the dispatch path (the LLM task spawns onto
/// it but, with no episodes, completes near-instantly via the fallback).
fn test_runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime"),
    )
}

/// `(minutes_since_midnight, day_of_week)` for the current local instant.
fn local_now() -> (u32, u8) {
    let now = Local::now();
    (now.hour() * 60 + now.minute(), now.weekday().num_days_from_sunday() as u8)
}

/// A schedule that fires at exactly the current local minute on the
/// current local weekday — i.e. "due right now".
fn schedule_due_now() -> BriefingSchedule {
    let (minutes, dow) = local_now();
    BriefingSchedule { time_of_day: minutes, days: vec![dow], enabled: true }
}

fn fresh_state() -> (
    Arc<Mutex<BriefingScheduler>>,
    Arc<Mutex<Option<BriefingSnapshot>>>,
    Arc<AtomicU64>,
    Arc<Mutex<PodcastStore>>,
    Arc<tokio::runtime::Runtime>,
) {
    (
        Arc::new(Mutex::new(BriefingScheduler::new())),
        Arc::new(Mutex::new(None)),
        Arc::new(AtomicU64::new(1)),
        Arc::new(Mutex::new(PodcastStore::new())),
        test_runtime(),
    )
}

#[test]
fn no_schedule_is_a_noop() {
    let (sched, slot, rev, store, rt) = fresh_state();
    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);
    assert!(slot.lock().unwrap().is_none(), "no slot written without a schedule");
    assert_eq!(rev.load(Ordering::Relaxed), 1, "rev untouched");
    assert!(sched.lock().unwrap().pending.is_none(), "no pending minted");
}

#[test]
fn disabled_schedule_is_a_noop() {
    let (sched, slot, rev, store, rt) = fresh_state();
    let mut s = schedule_due_now();
    s.enabled = false; // master switch off
    sched.lock().unwrap().set_schedule(s);

    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);
    assert!(slot.lock().unwrap().is_none());
    assert!(sched.lock().unwrap().pending.is_none());
}

#[test]
fn schedule_not_covering_today_is_a_noop() {
    let (sched, slot, rev, store, rt) = fresh_state();
    let (minutes, dow) = local_now();
    // Cover every weekday EXCEPT today.
    let days: Vec<u8> = (0u8..7).filter(|d| *d != dow).collect();
    sched.lock().unwrap().set_schedule(BriefingSchedule {
        time_of_day: minutes,
        days,
        enabled: true,
    });

    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);
    assert!(slot.lock().unwrap().is_none(), "today not covered → no fire");
    assert!(sched.lock().unwrap().pending.is_none());
}

#[test]
fn due_slot_mints_pending_and_writes_generating_snapshot() {
    let (sched, slot, rev, store, rt) = fresh_state();
    sched.lock().unwrap().set_schedule(schedule_due_now());

    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);

    // Scheduler latched: a pending briefing now exists.
    assert!(sched.lock().unwrap().pending.is_some(), "pending slot minted");

    // The existing generate-briefing path flipped the snapshot slot into
    // the generating state and bumped rev.
    let snap = slot.lock().unwrap().clone().expect("snapshot slot written");
    assert_eq!(snap.status, "generating");
    assert!(snap.is_generating);
    assert!(rev.load(Ordering::Relaxed) > 1, "rev bumped by dispatch");
}

#[test]
fn second_tick_does_not_refire_while_pending() {
    let (sched, slot, rev, store, rt) = fresh_state();
    sched.lock().unwrap().set_schedule(schedule_due_now());

    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);
    let rev_after_first = rev.load(Ordering::Relaxed);

    // Second tick in the same slot: the `pending` latch suppresses it.
    maybe_trigger_briefing(&sched, &slot, &rev, &store, &rt);
    assert_eq!(
        rev.load(Ordering::Relaxed),
        rev_after_first,
        "latch holds — no second dispatch while a briefing is pending",
    );
}

#[test]
fn next_scheduled_minutes_zero_when_due_now() {
    let (sched, _slot, _rev, _store, _rt) = fresh_state();
    sched.lock().unwrap().set_schedule(schedule_due_now());
    // Due exactly now → 0 minutes until the slot.
    assert_eq!(next_scheduled_minutes(&sched), Some(0));
}

#[test]
fn next_scheduled_minutes_none_without_schedule() {
    let (sched, _slot, _rev, _store, _rt) = fresh_state();
    assert_eq!(next_scheduled_minutes(&sched), None);
}

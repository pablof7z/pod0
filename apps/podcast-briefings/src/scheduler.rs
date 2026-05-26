//! Pure, clock-free briefing scheduler state machine.
//!
//! The scheduler holds at most one pending [`Briefing`] and the active
//! [`BriefingSchedule`]. It exposes a `should_generate_now` predicate
//! (caller supplies the wall clock) and a small set of state-transition
//! helpers the kernel-side action module calls when the composer /
//! stitcher report progress.
//!
//! ## Doctrine
//!
//! * **No `Utc::now()`.** Every wall-clock decision is parameterised by
//!   the caller. M9.B's `ActionModule` reads the clock once per tick
//!   and feeds it in; tests pass arbitrary values for deterministic
//!   coverage.
//! * **Single writer.** The scheduler owns `pending` — composer
//!   callbacks land here through [`Self::mark_generating`],
//!   [`Self::complete`], [`Self::fail`], and [`Self::deliver`].
//! * **No I/O.** Persistence lives in the surrounding `ActionModule`;
//!   the scheduler emits state, the module persists it.

use chrono::{DateTime, Utc};

use crate::types::{Briefing, BriefingSchedule, BriefingSegment, BriefingStatus};

/// Pure, synchronous briefing-scheduler projection.
///
/// Owns one optional pending briefing and the active schedule. All
/// state transitions are explicit method calls; the kernel decides
/// when to invoke them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BriefingScheduler {
    /// The current in-flight briefing, if any. The scheduler holds at
    /// most one briefing at a time — `complete` / `fail` / `deliver`
    /// drop it back to `None` so the next tick can mint a fresh slot.
    pub pending: Option<Briefing>,
    /// Active user-configured schedule. `None` before the user opens
    /// the Briefings settings; in that case `should_generate_now`
    /// always returns false.
    pub schedule: Option<BriefingSchedule>,
}

impl BriefingScheduler {
    /// Construct a fresh scheduler with no schedule and no pending
    /// briefing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when `(now_minutes_since_midnight, day_of_week)` falls
    /// inside the active schedule AND no briefing is currently pending
    /// (so the same slot doesn't spawn duplicates on every tick).
    ///
    /// `day_of_week` is **0 = Sunday … 6 = Saturday** (matches
    /// `BriefingSchedule::days`).
    ///
    /// The match is tolerant: any minute within the same minute as
    /// `time_of_day` is accepted. (The kernel ticks once per minute
    /// for the scheduler; if the tick lands at e.g. 07:00:30 the
    /// caller already truncated to `420`.)
    #[must_use]
    pub fn should_generate_now(
        &self,
        now_minutes_since_midnight: u32,
        day_of_week: u8,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let Some(sched) = self.schedule.as_ref() else {
            return false;
        };
        sched.covers(day_of_week) && sched.time_of_day == now_minutes_since_midnight
    }

    /// Set or replace the schedule. Existing `pending` briefings are
    /// untouched — they keep the schedule snapshotted on
    /// [`Briefing::pending`] at creation time.
    pub fn set_schedule(&mut self, schedule: BriefingSchedule) {
        self.schedule = Some(schedule);
    }

    /// Cancel the configured schedule outright. Existing `pending`
    /// briefings are untouched. The next `should_generate_now` call
    /// returns false until the schedule is restored.
    pub fn cancel_schedule(&mut self) {
        self.schedule = None;
    }

    /// Begin a new pending briefing in `Pending` state. Caller supplies
    /// the wall clock (D9 — kernel owns time). Returns a reference to
    /// the freshly-minted briefing for the action module to dispatch
    /// the composer call against.
    ///
    /// No-op (returns the existing briefing) when one is already
    /// pending — idempotent so a duplicated `RequestBriefing` action
    /// can't spawn two parallel composer runs.
    pub fn start_pending(&mut self, created_at: DateTime<Utc>) -> &Briefing {
        if self.pending.is_none() {
            let schedule = self.schedule.clone().unwrap_or_default();
            self.pending = Some(Briefing::pending(created_at, schedule));
        }
        // Unwrap-safe: we just inserted if it was None.
        self.pending.as_ref().expect("pending set above")
    }

    /// Transition `pending` from `Pending` to `Generating`. No-op when
    /// no pending briefing exists or the existing briefing is already
    /// past `Pending`. (Idempotence keeps composer-restart edge cases
    /// quiet.)
    pub fn mark_generating(&mut self) {
        if let Some(b) = self.pending.as_mut() {
            if matches!(b.status, BriefingStatus::Pending) {
                b.status = BriefingStatus::Generating;
            }
        }
    }

    /// Composer success — transition to `Ready` and populate
    /// `segments`. No-op when no pending briefing exists.
    pub fn complete(&mut self, segments: Vec<BriefingSegment>) {
        if let Some(b) = self.pending.as_mut() {
            b.segments = segments;
            b.status = BriefingStatus::Ready;
        }
    }

    /// Composer failure — transition to `Failed { error }`. No-op when
    /// no pending briefing exists. The action module persists the
    /// failure for diagnostics; the next scheduled slot mints a fresh
    /// briefing.
    pub fn fail(&mut self, error: String) {
        if let Some(b) = self.pending.as_mut() {
            b.status = BriefingStatus::failed(error);
        }
    }

    /// Mark the active briefing as delivered. Stamps `delivered_at`
    /// (caller-supplied per D9) and leaves the briefing in place so
    /// the snapshot can render a "today's briefing" entry until the
    /// next scheduling tick rotates it.
    ///
    /// No-op when no pending briefing exists, when the briefing is
    /// not in `Ready` state, or when it has already been delivered.
    pub fn deliver(&mut self, delivered_at: DateTime<Utc>) {
        if let Some(b) = self.pending.as_mut() {
            if matches!(b.status, BriefingStatus::Ready) && b.delivered_at.is_none() {
                b.status = BriefingStatus::Delivered;
                b.delivered_at = Some(delivered_at);
            }
        }
    }

    /// Drop the current pending briefing entirely. Used when the user
    /// explicitly cancels (`podcast.briefing.cancel`) or when a fresh
    /// scheduling tick wants to retire the old slot before minting a
    /// new one.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    /// Minutes until the next scheduled briefing on the same calendar
    /// day, given `now_minutes_since_midnight` and `day_of_week`.
    /// `None` when no schedule is active, when the schedule doesn't
    /// cover today, or when the slot has already passed.
    ///
    /// Pure projection — the snapshot encoder calls this to populate
    /// `next_scheduled_minutes`.
    #[must_use]
    pub fn next_scheduled_minutes(
        &self,
        now_minutes_since_midnight: u32,
        day_of_week: u8,
    ) -> Option<u32> {
        let s = self.schedule.as_ref()?;
        if !s.covers(day_of_week) {
            return None;
        }
        s.time_of_day.checked_sub(now_minutes_since_midnight)
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;

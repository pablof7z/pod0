//! Unit tests for [`super::PlayerActor`].
//!
//! Split by concern so each module stays comfortably under the 300-LOC
//! soft cap:
//!
//! * [`reports`] — AudioReport-by-variant projection tests
//!   (`Playing`, `Paused`, `Stopped`, `Failed`, `BufferingProgress`).
//! * [`sleep_timer`] — D9 sleep-timer expiry semantics
//!   (arming, expiry mid-Playing, `SleepTimerFired`, cancel).
//! * [`mutators`] — Direct state mutators
//!   (`stage_load`, `set_speed`, `set_volume`) and default constructors.
//! * [`queue`] — Playback queue ("Up Next") mutators
//!   (`enqueue`, `dequeue`, `clear_queue`, `pop_next`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod mutators;
mod queue;
mod reports;
mod sleep_timer;

/// A fixed instant used as `now` in every test where the absolute value
/// doesn't matter — only the relative offsets to the sleep-timer
/// deadline.
pub(super) fn t0() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

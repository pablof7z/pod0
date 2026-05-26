//! Tests for [`super::scheduler`] — BriefingScheduler lifecycle and schedule coverage.
//!
//! Extracted from `scheduler.rs` to keep that file under the 500-line hard limit.

use super::*;
use chrono::TimeZone;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 7, 0, 0).unwrap()
}

fn weekdays_at_7am() -> BriefingSchedule {
    BriefingSchedule {
        time_of_day: 420,
        days: vec![1, 2, 3, 4, 5],
        enabled: true,
    }
}

// ── should_generate_now ────────────────────────────────────────────

#[test]
fn should_generate_now_false_without_schedule() {
    let s = BriefingScheduler::new();
    assert!(!s.should_generate_now(420, 1));
}

#[test]
fn should_generate_now_true_when_time_and_day_match() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    assert!(s.should_generate_now(420, 1)); // Monday 07:00
}

#[test]
fn should_generate_now_false_on_wrong_minute() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    assert!(!s.should_generate_now(419, 1));
    assert!(!s.should_generate_now(421, 1));
}

#[test]
fn should_generate_now_false_on_excluded_day() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    assert!(!s.should_generate_now(420, 0)); // Sunday
    assert!(!s.should_generate_now(420, 6)); // Saturday
}

#[test]
fn should_generate_now_false_when_disabled() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(BriefingSchedule {
        time_of_day: 420,
        days: vec![1],
        enabled: false,
    });
    assert!(!s.should_generate_now(420, 1));
}

#[test]
fn should_generate_now_false_when_pending_exists() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    s.start_pending(t0());
    assert!(!s.should_generate_now(420, 1));
}

// ── lifecycle transitions ──────────────────────────────────────────

#[test]
fn start_pending_creates_briefing_in_pending_state() {
    let mut s = BriefingScheduler::new();
    let b = s.start_pending(t0()).clone();
    assert_eq!(b.status, BriefingStatus::Pending);
    assert!(b.segments.is_empty());
    assert_eq!(b.created_at, t0());
}

#[test]
fn start_pending_is_idempotent() {
    let mut s = BriefingScheduler::new();
    let first = s.start_pending(t0()).id;
    let second = s.start_pending(t0()).id;
    assert_eq!(first, second);
}

#[test]
fn mark_generating_transitions_pending_to_generating() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    s.mark_generating();
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Generating);
}

#[test]
fn complete_transitions_to_ready_and_populates_segments() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    s.mark_generating();
    let segs = vec![BriefingSegment::new(crate::types::SegmentKind::Intro, "good morning")];
    s.complete(segs.clone());
    let b = s.pending.as_ref().unwrap();
    assert_eq!(b.status, BriefingStatus::Ready);
    assert_eq!(b.segments, segs);
}

#[test]
fn fail_transitions_to_failed_with_error() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    s.fail("boom".into());
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::failed("boom"));
}

#[test]
fn deliver_only_transitions_from_ready() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    // From Pending — no-op.
    s.deliver(t0());
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Pending);
    // From Ready — transitions.
    s.complete(vec![]);
    let later = t0() + chrono::Duration::minutes(5);
    s.deliver(later);
    let b = s.pending.as_ref().unwrap();
    assert_eq!(b.status, BriefingStatus::Delivered);
    assert_eq!(b.delivered_at, Some(later));
}

#[test]
fn deliver_is_idempotent() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    s.complete(vec![]);
    s.deliver(t0());
    let first = s.pending.as_ref().unwrap().delivered_at;
    s.deliver(t0() + chrono::Duration::minutes(10));
    // Second call must not overwrite the timestamp.
    assert_eq!(s.pending.as_ref().unwrap().delivered_at, first);
}

#[test]
fn clear_pending_drops_briefing() {
    let mut s = BriefingScheduler::new();
    s.start_pending(t0());
    s.clear_pending();
    assert!(s.pending.is_none());
}

// ── projections ────────────────────────────────────────────────────

#[test]
fn next_scheduled_minutes_returns_delta_until_slot() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    assert_eq!(s.next_scheduled_minutes(360, 1), Some(60));
}

#[test]
fn next_scheduled_minutes_none_when_slot_passed() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    // 8 am, slot was 7 am — already passed.
    assert!(s.next_scheduled_minutes(480, 1).is_none());
}

#[test]
fn next_scheduled_minutes_none_when_day_not_covered() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    assert!(s.next_scheduled_minutes(360, 0).is_none());
}

// ── canonical lifecycle ────────────────────────────────────────────

#[test]
fn full_lifecycle_pending_generating_ready_delivered() {
    let mut s = BriefingScheduler::new();
    s.set_schedule(weekdays_at_7am());
    s.start_pending(t0());
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Pending);
    s.mark_generating();
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Generating);
    s.complete(vec![BriefingSegment::new(crate::types::SegmentKind::Intro, "good morning")]);
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Ready);
    s.deliver(t0() + chrono::Duration::minutes(2));
    assert_eq!(s.pending.as_ref().unwrap().status, BriefingStatus::Delivered);
}

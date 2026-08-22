use std::{
    thread,
    time::{Duration, Instant, SystemTime},
};

use pod0_system_hosts::{WakeDeadline, WakeOutcome, WakeTimer};

#[test]
fn monotonic_timer_really_waits_and_returns_firing_evidence() {
    let delay = Duration::from_millis(40);
    let started = Instant::now();
    let timer = WakeTimer::schedule(WakeDeadline::After(delay)).unwrap();

    let outcome = timer.wait().unwrap();

    let WakeOutcome::Fired(evidence) = outcome else {
        panic!("timer should fire");
    };
    assert!(started.elapsed() >= delay);
    assert!(evidence.elapsed_monotonic >= delay);
    assert!(evidence.observed_at.monotonic >= evidence.plan.target_monotonic);
}

#[test]
fn cancellation_wakes_a_real_long_timer_promptly() {
    let timer = WakeTimer::schedule(WakeDeadline::After(Duration::from_secs(5))).unwrap();
    let cancellation = timer.cancellation();
    thread::sleep(Duration::from_millis(30));
    let cancelled_at = Instant::now();

    assert!(cancellation.cancel());
    let outcome = timer.wait().unwrap();

    let WakeOutcome::Cancelled(evidence) = outcome else {
        panic!("timer should be cancelled");
    };
    assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    assert!(evidence.elapsed_monotonic < Duration::from_secs(1));
    assert!(cancellation.is_cancelled());
}

#[test]
fn past_wall_deadline_fires_immediately_with_wall_and_monotonic_evidence() {
    let requested = SystemTime::now() - Duration::from_secs(1);
    let timer = WakeTimer::schedule(WakeDeadline::At(requested)).unwrap();

    let outcome = timer.wait().unwrap();

    let WakeOutcome::Fired(evidence) = outcome else {
        panic!("past wall deadline should fire");
    };
    assert_eq!(evidence.plan.target_wall, requested);
    assert!(evidence.elapsed_monotonic < Duration::from_secs(1));
    assert!(evidence.observed_at.wall >= evidence.plan.scheduled_at.wall);
}

#[test]
fn future_wall_deadline_waits_until_the_requested_wall_time() {
    let requested = SystemTime::now() + Duration::from_millis(40);
    let timer = WakeTimer::schedule(WakeDeadline::At(requested)).unwrap();

    let outcome = timer.wait().unwrap();

    let WakeOutcome::Fired(evidence) = outcome else {
        panic!("wall timer should fire");
    };
    assert_eq!(evidence.plan.target_wall, requested);
    assert!(evidence.observed_at.wall >= requested);
    assert!(evidence.elapsed_monotonic >= Duration::from_millis(30));
}

#[test]
fn fired_timer_cannot_be_relabelled_cancelled_by_drop_or_a_late_cancel() {
    let timer = WakeTimer::schedule(WakeDeadline::After(Duration::ZERO)).unwrap();
    let cancellation = timer.cancellation();

    let outcome = timer.wait().unwrap();

    assert!(matches!(outcome, WakeOutcome::Fired(_)));
    assert!(!cancellation.cancel());
    assert!(!cancellation.is_cancelled());
}

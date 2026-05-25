//! Sleep-timer expiry tests — D9: kernel decides expiry.
//!
//! The actor exposes `arm_sleep_timer` and `cancel_sleep_timer`; the
//! authoritative expiry check happens inside `handle_audio_report` so
//! ticks past the deadline emit `AudioCommand::Stop` even if the iOS
//! side never explicitly sends `SleepTimerFired`.

use std::time::Duration;

use crate::capability::{AudioCommand, AudioReport};
use crate::player::PlayerActor;

use super::t0;

#[test]
fn sleep_timer_expires_on_playing_report_after_deadline() {
    let mut actor = PlayerActor::new();
    let now = t0();
    actor.arm_sleep_timer(Duration::from_secs(30), now);
    // First tick well before expiry: no command.
    let cmd = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 1800.0,
        },
        now + Duration::from_secs(5),
    );
    assert!(cmd.is_none());
    assert_eq!(
        actor.state().sleep_timer_remaining_secs,
        Some(25),
        "countdown surfaces to the projection"
    );

    // Tick past expiry: Stop emitted, deadline cleared.
    let cmd = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 31.0,
            duration_secs: 1800.0,
        },
        now + Duration::from_secs(31),
    );
    assert_eq!(cmd, Some(AudioCommand::Stop));
    assert!(actor.sleep_deadline().is_none());
    assert!(actor.state().sleep_timer_remaining_secs.is_none());
}

#[test]
fn sleep_timer_fired_report_emits_stop_and_clears_deadline() {
    let mut actor = PlayerActor::new();
    actor.arm_sleep_timer(Duration::from_secs(60), t0());
    let cmd = actor.handle_audio_report(AudioReport::SleepTimerFired, t0());
    assert_eq!(cmd, Some(AudioCommand::Stop));
    assert!(actor.sleep_deadline().is_none());
    assert!(actor.state().sleep_timer_remaining_secs.is_none());
}

#[test]
fn cancel_sleep_timer_clears_deadline_and_remaining() {
    let mut actor = PlayerActor::new();
    actor.arm_sleep_timer(Duration::from_secs(60), t0());
    actor.cancel_sleep_timer();
    assert!(actor.sleep_deadline().is_none());
    assert!(actor.state().sleep_timer_remaining_secs.is_none());
    // A subsequent Playing report well past the original deadline must
    // not trip Stop — the timer is gone.
    let cmd = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 0.0,
            duration_secs: 1000.0,
        },
        t0() + Duration::from_secs(120),
    );
    assert!(cmd.is_none());
}

#[test]
fn hard_stop_clears_sleep_timer() {
    let mut actor = PlayerActor::new();
    actor.arm_sleep_timer(Duration::from_secs(60), t0());
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 10.0,
        },
        t0(),
    );
    let _ = actor.handle_audio_report(AudioReport::Stopped, t0());
    assert!(actor.sleep_deadline().is_none(), "hard stop clears timer");
    assert!(actor.state().sleep_timer_remaining_secs.is_none());
}

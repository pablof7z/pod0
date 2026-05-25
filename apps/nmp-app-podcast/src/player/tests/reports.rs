//! AudioReport-by-variant projection tests.
//!
//! Every report variant feeds into `PlayerActor::handle_audio_report`
//! exactly once and the resulting `PlayerState` is asserted. No timer
//! interaction here — see `super::sleep_timer` for that.

use crate::capability::AudioReport;
use crate::player::PlayerActor;

use super::t0;

// ---------------------------------------------------------------------------
// AudioReport::Playing
// ---------------------------------------------------------------------------

#[test]
fn playing_report_updates_position_duration_and_flag() {
    let mut actor = PlayerActor::new();
    let cmd = actor.handle_audio_report(
        AudioReport::Playing {
            url: "https://ex.com/ep.mp3".into(),
            position_secs: 12.5,
            duration_secs: 1800.0,
        },
        t0(),
    );
    assert!(cmd.is_none(), "no follow-up command expected");
    assert!(actor.state().is_playing);
    assert_eq!(actor.state().position_secs, 12.5);
    assert_eq!(actor.state().duration_secs, 1800.0);
    assert_eq!(actor.state().url.as_deref(), Some("https://ex.com/ep.mp3"));
    assert!(actor.state().last_error.is_none());
}

#[test]
fn playing_report_keeps_prior_duration_when_unknown() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 0.0,
            duration_secs: 1800.0,
        },
        t0(),
    );
    // A subsequent report with `duration_secs == 0.0` (e.g. live stream)
    // must not clobber the resolved duration.
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 0.0,
        },
        t0(),
    );
    assert_eq!(actor.state().duration_secs, 1800.0);
    assert_eq!(actor.state().position_secs, 1.0);
}

#[test]
fn playing_report_clamps_negative_position_to_zero() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: -3.0,
            duration_secs: 10.0,
        },
        t0(),
    );
    assert_eq!(actor.state().position_secs, 0.0);
}

#[test]
fn playing_report_clears_buffering_fraction() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(AudioReport::BufferingProgress { fraction: 0.4 }, t0());
    assert_eq!(actor.state().buffering_fraction, Some(0.4));
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 10.0,
        },
        t0(),
    );
    assert!(actor.state().buffering_fraction.is_none());
}

// ---------------------------------------------------------------------------
// AudioReport::Paused / Stopped
// ---------------------------------------------------------------------------

#[test]
fn paused_report_flips_is_playing_off_and_records_position() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 100.0,
            duration_secs: 1000.0,
        },
        t0(),
    );
    let cmd = actor.handle_audio_report(
        AudioReport::Paused {
            url: "u".into(),
            position_secs: 101.5,
        },
        t0(),
    );
    assert!(cmd.is_none());
    assert!(!actor.state().is_playing);
    assert_eq!(actor.state().position_secs, 101.5);
}

#[test]
fn stopped_report_clears_playing_and_buffering() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 10.0,
        },
        t0(),
    );
    let cmd = actor.handle_audio_report(AudioReport::Stopped, t0());
    assert!(cmd.is_none());
    assert!(!actor.state().is_playing);
    assert!(actor.state().buffering_fraction.is_none());
}

// ---------------------------------------------------------------------------
// AudioReport::Failed
// ---------------------------------------------------------------------------

#[test]
fn failed_report_records_error_and_stops_playing() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 10.0,
        },
        t0(),
    );
    let cmd = actor.handle_audio_report(
        AudioReport::Failed {
            url: "u".into(),
            error: "transport: timeout".into(),
        },
        t0(),
    );
    assert!(cmd.is_none());
    assert!(!actor.state().is_playing);
    assert_eq!(
        actor.state().last_error.as_deref(),
        Some("transport: timeout")
    );
}

// ---------------------------------------------------------------------------
// AudioReport::BufferingProgress
// ---------------------------------------------------------------------------

#[test]
fn buffering_progress_clamps_to_unit_range() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(AudioReport::BufferingProgress { fraction: 1.5 }, t0());
    assert_eq!(actor.state().buffering_fraction, Some(1.0));
    let _ = actor.handle_audio_report(AudioReport::BufferingProgress { fraction: -0.2 }, t0());
    assert_eq!(actor.state().buffering_fraction, Some(0.0));
}

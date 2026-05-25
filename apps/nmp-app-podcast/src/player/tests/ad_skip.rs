//! Ad auto-skip behaviour for [`super::super::PlayerActor`].
//!
//! Test grid:
//!   * disabled toggle ⇒ never emits a `Seek` even when the playhead
//!     is inside a segment;
//!   * enabled + playhead-in-segment ⇒ exactly one `Seek` to
//!     `end_secs`, segment id remembered;
//!   * second `Playing` tick still inside the same segment ⇒ no
//!     re-skip (idempotent per session);
//!   * `Stopped` clears the session memory ⇒ same segment is
//!     skippable again on a fresh playback;
//!   * sleep-timer expiry takes precedence over ad-skip (we don't
//!     want to seek-then-stop in one tick);
//!   * `set_ad_segments` resets the skipped-id set so a fresh ingest
//!     of the same id is honoured.

use std::time::{Duration, SystemTime};

use podcast_core::{AdKind, AdSegment};

use crate::capability::{AudioCommand, AudioReport};
use crate::player::PlayerActor;

use super::t0;

/// Helper: build an [`AdSegment`] (id is auto-assigned). Tests that
/// need to assert against a specific id should capture the returned
/// value's `.id` field.
fn seg(start: f64, end: f64) -> AdSegment {
    AdSegment::new(start, end, AdKind::Midroll)
}

fn playing(position_secs: f64) -> AudioReport {
    AudioReport::Playing {
        url: "https://ex.com/ep.mp3".into(),
        position_secs,
        duration_secs: 1800.0,
    }
}

#[test]
fn defaults_are_disabled_with_empty_segments() {
    let actor = PlayerActor::new();
    assert!(!actor.auto_skip_ads());
    assert!(actor.ad_segments().is_empty());
}

#[test]
fn no_skip_when_toggle_is_off() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    // Toggle deliberately left at default `false`.
    let cmd = actor.handle_audio_report(playing(45.0), t0());
    assert!(cmd.is_none(), "no skip when toggle off");
}

#[test]
fn no_skip_when_segments_empty() {
    let mut actor = PlayerActor::new();
    actor.set_auto_skip_ads(true);
    let cmd = actor.handle_audio_report(playing(45.0), t0());
    assert!(cmd.is_none());
}

#[test]
fn skips_when_inside_a_segment_and_toggle_on() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    let cmd = actor
        .handle_audio_report(playing(45.0), t0())
        .expect("expected a follow-up Seek");
    match cmd {
        AudioCommand::Seek { position_secs } => assert_eq!(position_secs, 60.0),
        other => panic!("expected Seek, got {other:?}"),
    }
}

#[test]
fn does_not_skip_when_outside_segment() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    let cmd = actor.handle_audio_report(playing(15.0), t0());
    assert!(cmd.is_none(), "outside segment ⇒ no skip");
}

#[test]
fn does_not_re_skip_same_segment_within_session() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    // First tick inside the segment ⇒ skip emitted.
    let cmd = actor.handle_audio_report(playing(45.0), t0());
    assert!(cmd.is_some());

    // User scrubs back into the same ad; we must NOT skip again.
    let cmd2 = actor.handle_audio_report(playing(40.0), t0());
    assert!(cmd2.is_none(), "scrub-back into already-skipped ad is a deliberate listen");
}

#[test]
fn skips_each_distinct_segment_once() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![
        seg(30.0, 60.0),
        seg(120.0, 150.0),
    ]);
    actor.set_auto_skip_ads(true);

    let cmd1 = actor.handle_audio_report(playing(45.0), t0());
    assert!(matches!(cmd1, Some(AudioCommand::Seek { position_secs }) if position_secs == 60.0));

    let cmd2 = actor.handle_audio_report(playing(130.0), t0());
    assert!(matches!(cmd2, Some(AudioCommand::Seek { position_secs }) if position_secs == 150.0));
}

#[test]
fn stopped_clears_session_memory_and_allows_re_skip() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    // First session: skip the ad.
    let _ = actor.handle_audio_report(playing(45.0), t0());
    // End the session.
    let _ = actor.handle_audio_report(AudioReport::Stopped, t0());

    // Fresh session, same actor, same ad list — should skip again.
    let cmd = actor.handle_audio_report(playing(45.0), t0());
    assert!(
        matches!(cmd, Some(AudioCommand::Seek { position_secs }) if position_secs == 60.0),
        "Stopped should reset skipped-ad memory",
    );
}

#[test]
fn set_ad_segments_resets_skipped_set() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    let _ = actor.handle_audio_report(playing(45.0), t0());

    // Re-ingest the same id — caller's contract is "this is the
    // canonical list now", so the skipped set must be cleared so the
    // new list is fully eligible (e.g. publisher republished with
    // adjusted bounds).
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    let cmd = actor.handle_audio_report(playing(45.0), t0());
    assert!(cmd.is_some(), "set_ad_segments should reset skip memory");
}

#[test]
fn sleep_timer_expiry_takes_precedence_over_ad_skip() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);
    // Arm a timer that has already elapsed by the time this tick lands.
    actor.arm_sleep_timer(Duration::from_secs(1), t0());
    let later = t0() + Duration::from_secs(120);

    let cmd = actor
        .handle_audio_report(playing(45.0), later)
        .expect("expected a follow-up command");
    assert!(
        matches!(cmd, AudioCommand::Stop),
        "sleep-timer expiry must short-circuit ad-skip",
    );
}

#[test]
fn toggle_off_after_skip_does_not_replay_skip_on_re_enable() {
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(30.0, 60.0)]);
    actor.set_auto_skip_ads(true);

    // Skip once.
    let _ = actor.handle_audio_report(playing(45.0), t0());

    // User toggles off mid-playback.
    actor.set_auto_skip_ads(false);
    let cmd_off = actor.handle_audio_report(playing(40.0), t0());
    assert!(cmd_off.is_none());

    // User toggles back on; cursor still inside the same (already-skipped) ad.
    actor.set_auto_skip_ads(true);
    let cmd_on = actor.handle_audio_report(playing(40.0), t0());
    assert!(cmd_on.is_none(), "re-enable must not replay an already-dismissed skip");
}

#[test]
fn ad_segment_at_zero_start_is_handled() {
    // Some publishers run a pre-roll starting at exactly 0.0.
    let mut actor = PlayerActor::new();
    actor.set_ad_segments(vec![seg(0.0, 15.0)]);
    actor.set_auto_skip_ads(true);

    // SystemTime is a fixed instant — only the actor's own state matters.
    let cmd = actor.handle_audio_report(playing(0.0), SystemTime::UNIX_EPOCH + Duration::from_secs(1));
    assert!(matches!(cmd, Some(AudioCommand::Seek { position_secs }) if position_secs == 15.0));
}

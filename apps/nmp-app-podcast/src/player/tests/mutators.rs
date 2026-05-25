//! Direct state-mutator tests — `stage_load`, `set_speed`, `set_volume`,
//! and the default constructors.

use crate::capability::AudioReport;
use crate::player::{PlayerActor, PlayerState};

use super::t0;

#[test]
fn idle_state_has_neutral_rate_and_volume() {
    let state = PlayerState::idle();
    assert_eq!(state.speed, 1.0);
    assert_eq!(state.volume, 1.0);
    assert!(state.episode_id.is_none());
    assert!(!state.is_playing);
}

#[test]
fn new_actor_starts_idle_with_no_deadline() {
    let actor = PlayerActor::new();
    assert!(actor.sleep_deadline().is_none());
    assert!(!actor.state().is_playing);
    assert_eq!(actor.state().speed, 1.0);
}

#[test]
fn stage_load_records_metadata_and_initial_position() {
    let mut actor = PlayerActor::new();
    actor.stage_load(
        "ep-42",
        Some("show-9".into()),
        "https://ex.com/ep-42.mp3",
        300.0,
    );
    assert_eq!(actor.state().episode_id.as_deref(), Some("ep-42"));
    assert_eq!(actor.state().podcast_id.as_deref(), Some("show-9"));
    assert_eq!(
        actor.state().url.as_deref(),
        Some("https://ex.com/ep-42.mp3")
    );
    assert_eq!(actor.state().position_secs, 300.0);
    assert!(!actor.state().is_playing);
}

#[test]
fn next_load_clears_prior_error() {
    let mut actor = PlayerActor::new();
    let _ = actor.handle_audio_report(
        AudioReport::Failed {
            url: "u".into(),
            error: "boom".into(),
        },
        t0(),
    );
    actor.stage_load("ep1", Some("show1".into()), "u2", 0.0);
    assert!(actor.state().last_error.is_none());
    assert_eq!(actor.state().episode_id.as_deref(), Some("ep1"));
    assert_eq!(actor.state().podcast_id.as_deref(), Some("show1"));
    assert_eq!(actor.state().url.as_deref(), Some("u2"));
}

#[test]
fn set_speed_clamps_to_valid_range() {
    let mut actor = PlayerActor::new();
    actor.set_speed(3.0);
    assert_eq!(actor.state().speed, 2.0);
    actor.set_speed(0.1);
    assert_eq!(actor.state().speed, 0.5);
    actor.set_speed(1.25);
    assert_eq!(actor.state().speed, 1.25);
}

#[test]
fn set_volume_clamps_to_unit_range() {
    let mut actor = PlayerActor::new();
    actor.set_volume(1.5);
    assert_eq!(actor.state().volume, 1.0);
    actor.set_volume(-0.5);
    assert_eq!(actor.state().volume, 0.0);
    actor.set_volume(0.4);
    assert_eq!(actor.state().volume, 0.4);
}

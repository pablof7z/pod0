//! Tests for [`super::build_widget_snapshot`] — the kernel-owned widget
//! projection (D4 single source of truth).

use super::build_widget_snapshot;
use crate::ffi::projections::{EpisodeSummary, PodcastSummary};
use crate::player::PlayerState;

/// A subscribed show with the given episodes and per-show unplayed count.
fn show(id: &str, title: &str, unplayed: usize, episodes: Vec<EpisodeSummary>) -> PodcastSummary {
    PodcastSummary {
        id: id.into(),
        title: title.into(),
        unplayed_count: unplayed,
        is_subscribed: true,
        episodes,
        ..Default::default()
    }
}

fn episode(id: &str, title: &str) -> EpisodeSummary {
    EpisodeSummary {
        id: id.into(),
        title: title.into(),
        ..Default::default()
    }
}

fn playing(episode_id: &str, position: f64, duration: f64) -> PlayerState {
    PlayerState {
        episode_id: Some(episode_id.into()),
        position_secs: position,
        duration_secs: duration,
        is_playing: true,
        ..PlayerState::idle()
    }
}

#[test]
fn no_episode_and_no_unplayed_yields_none() {
    // Empty library, nothing playing → nothing to surface → None (host clears
    // the App Group key; the widget renders its empty state).
    assert!(build_widget_snapshot(None, &[]).is_none());

    // A subscribed show with zero unplayed and nothing playing is still None.
    let lib = vec![show("p1", "Show", 0, vec![episode("e1", "Ep")])];
    assert!(build_widget_snapshot(None, &lib).is_none());
}

#[test]
fn unplayed_only_yields_empty_now_playing_with_badge() {
    // Nothing playing, but there ARE unplayed episodes → Some with empty
    // now-playing fields and a non-zero badge so the widget can render
    // "N to listen" without a hero.
    let lib = vec![show("p1", "Show", 3, vec![episode("e1", "Ep")])];
    let widget = build_widget_snapshot(None, &lib).expect("badge-only widget");
    assert_eq!(widget.now_playing_episode_title, None);
    assert_eq!(widget.now_playing_podcast_title, None);
    assert_eq!(widget.now_playing_artwork_url, None);
    assert_eq!(widget.now_playing_chapter_title, None);
    assert!(!widget.is_playing);
    assert_eq!(widget.position_fraction, 0.0);
    assert_eq!(widget.position_secs, 0.0);
    assert_eq!(widget.duration_secs, 0.0);
    assert_eq!(widget.unplayed_count, 3);
}

#[test]
fn playing_episode_resolves_title_show_and_fraction() {
    let ep = EpisodeSummary {
        id: "e1".into(),
        title: "Episode One".into(),
        artwork_url: Some("https://ex.com/ep.png".into()),
        ..Default::default()
    };
    let lib = vec![show("p1", "Great Show", 2, vec![ep])];
    let state = playing("e1", 30.0, 120.0);

    let widget = build_widget_snapshot(Some(&state), &lib).expect("playing widget");
    assert_eq!(widget.now_playing_episode_title.as_deref(), Some("Episode One"));
    assert_eq!(widget.now_playing_podcast_title.as_deref(), Some("Great Show"));
    assert_eq!(widget.now_playing_artwork_url.as_deref(), Some("https://ex.com/ep.png"));
    assert!(widget.is_playing);
    assert_eq!(widget.position_secs, 30.0);
    assert_eq!(widget.duration_secs, 120.0);
    assert!((widget.position_fraction - 0.25).abs() < 1e-6);
    assert_eq!(widget.unplayed_count, 2);
}

#[test]
fn artwork_falls_back_to_show_when_episode_has_none() {
    let ep = episode("e1", "Ep"); // no episode artwork
    let mut podcast = show("p1", "Show", 0, vec![ep]);
    podcast.artwork_url = Some("https://ex.com/show.png".into());
    let lib = vec![podcast];
    let state = playing("e1", 0.0, 60.0);

    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(widget.now_playing_artwork_url.as_deref(), Some("https://ex.com/show.png"));
}

#[test]
fn fraction_clamped_on_zero_duration() {
    let lib = vec![show("p1", "Show", 0, vec![episode("e1", "Ep")])];
    // Duration 0 (capability hasn't reported it) → fraction 0.0, no div-by-zero.
    let state = playing("e1", 42.0, 0.0);
    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(widget.position_fraction, 0.0);
    // Raw secs are still carried so the widget label logic owns the fallback.
    assert_eq!(widget.position_secs, 42.0);
    assert_eq!(widget.duration_secs, 0.0);
}

#[test]
fn fraction_clamped_when_position_exceeds_duration() {
    let lib = vec![show("p1", "Show", 0, vec![episode("e1", "Ep")])];
    // Position past the end (stale playhead) clamps to 1.0, never > 1.0.
    let state = playing("e1", 500.0, 100.0);
    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(widget.position_fraction, 1.0);
}

#[test]
fn library_duration_used_when_player_duration_unknown() {
    // Feed metadata carries a duration; the player hasn't reported one yet
    // (duration 0). The widget should use the feed duration so its
    // remaining-time label is correct before playback engages.
    let ep = EpisodeSummary {
        id: "e1".into(),
        title: "Ep".into(),
        duration_secs: Some(600.0),
        ..Default::default()
    };
    let lib = vec![show("p1", "Show", 0, vec![ep])];
    let state = playing("e1", 150.0, 0.0); // player duration unknown
    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(widget.duration_secs, 600.0);
    assert!((widget.position_fraction - 0.25).abs() < 1e-6);
}

#[test]
fn player_duration_preferred_over_library_when_known() {
    // Once the player reports a real duration it wins (it's the authoritative
    // engine value); the feed estimate is only a pre-playback fallback.
    let ep = EpisodeSummary {
        id: "e1".into(),
        title: "Ep".into(),
        duration_secs: Some(600.0),
        ..Default::default()
    };
    let lib = vec![show("p1", "Show", 0, vec![ep])];
    let state = playing("e1", 100.0, 400.0);
    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(widget.duration_secs, 400.0);
}

#[test]
fn chapter_title_carried_through() {
    let lib = vec![show("p1", "Show", 0, vec![episode("e1", "Ep")])];
    let mut state = playing("e1", 10.0, 100.0);
    state.current_chapter_title = Some("Chapter 3: The Reveal".into());
    let widget = build_widget_snapshot(Some(&state), &lib).expect("widget");
    assert_eq!(
        widget.now_playing_chapter_title.as_deref(),
        Some("Chapter 3: The Reveal")
    );
}

#[test]
fn unplayed_count_only_sums_subscribed_shows() {
    let subscribed = show("p1", "Followed", 4, vec![episode("e1", "Ep")]);
    let mut unfollowed = show("p2", "Ingested", 99, vec![episode("e2", "Ep2")]);
    unfollowed.is_subscribed = false;
    let lib = vec![subscribed, unfollowed];
    // Nothing playing but 4 unplayed in the followed show → badge = 4, the
    // unfollowed show's 99 is excluded.
    let widget = build_widget_snapshot(None, &lib).expect("widget");
    assert_eq!(widget.unplayed_count, 4);
}

#[test]
fn unplayed_count_sums_across_multiple_subscribed_shows() {
    let lib = vec![
        show("p1", "A", 2, vec![]),
        show("p2", "B", 3, vec![]),
        show("p3", "C", 0, vec![]),
    ];
    let widget = build_widget_snapshot(None, &lib).expect("widget");
    assert_eq!(widget.unplayed_count, 5);
}

#[test]
fn playing_episode_absent_from_library_falls_back_to_id() {
    // Streaming an external episode not in the followed library: the widget
    // still renders (never a blank face while playing) using the id as title.
    let state = playing("ghost-ep", 5.0, 50.0);
    let widget = build_widget_snapshot(Some(&state), &[]).expect("widget");
    assert_eq!(widget.now_playing_episode_title.as_deref(), Some("ghost-ep"));
    assert_eq!(widget.now_playing_podcast_title, None);
    assert!(widget.is_playing);
}

#[test]
fn idle_player_state_with_no_episode_treated_as_not_loaded() {
    // PlayerState::idle() has episode_id = None; with no unplayed episodes the
    // result is None (the `episode_id.is_some()` filter rejects idle states).
    let state = PlayerState::idle();
    assert!(build_widget_snapshot(Some(&state), &[]).is_none());
}

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

// ── Cross-language wire fixture ─────────────────────────────────────
//
// `tests/fixtures/podcast_update_with_widget.json` is a *Rust-emitted*
// `PodcastUpdate` JSON (a populated `widget` embedded in a non-empty
// `library` — the exact frame shape the bridge decodes). The Swift
// `PlatformWidgetContractTests` decodes the SAME bytes through the bridge's
// `keyDecodingStrategy = .convertFromSnakeCase` config and asserts the widget
// + library survive. That pairing is what would have caught the PR #366
// regression (explicit snake_case `CodingKeys` on the embedded `WidgetSnapshot`
// double-converted under the bridge strategy → `is_playing` threw `keyNotFound`
// → the *entire* PodcastUpdate decode failed → the library froze empty).
//
// Building the fixture here (not hand-typing it) guarantees the bytes are real
// serde output; the parity assert fails if the wire shape ever drifts.

/// The canonical `PodcastUpdate` the wire-fixture tests pin: one subscribed
/// show with one episode, actively playing, with a fully-populated widget.
fn fixture_update() -> crate::ffi::snapshot_update::PodcastUpdate {
    use crate::ffi::snapshot_update::PodcastUpdate;

    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "The Daily — Friday".into(),
        artwork_url: Some("https://ex.com/ep.png".into()),
        duration_secs: Some(1200.0),
        ..Default::default()
    };
    let mut state = playing("ep-1", 300.0, 1200.0);
    state.current_chapter_title = Some("Headlines".into());
    let library = vec![show("show-1", "The Daily", 5, vec![ep])];
    let widget = build_widget_snapshot(Some(&state), &library);
    assert!(widget.is_some(), "fixture must have a populated widget");

    PodcastUpdate {
        running: true,
        rev: 1,
        schema_version: 1,
        library,
        widget,
        ..PodcastUpdate::default()
    }
}

#[test]
fn podcast_update_with_widget_matches_fixture() {
    let fixture = include_str!("../../../../tests/fixtures/podcast_update_with_widget.json");
    let actual = serde_json::to_string_pretty(&fixture_update()).expect("encode");
    assert_eq!(
        actual.trim(),
        fixture.trim(),
        "PodcastUpdate wire shape drifted from \
         tests/fixtures/podcast_update_with_widget.json.\n\
         If this change is intentional, regenerate the fixture:\n\
         \tcargo test -p nmp-app-podcast regenerate_podcast_update_widget_fixture -- --ignored --nocapture\n\
         The Swift PlatformWidgetContractTests decodes this exact JSON through the \
         bridge decoder, so keep them in sync."
    );
}

/// Regeneration helper for [`podcast_update_with_widget_matches_fixture`].
/// Ignored by default; run explicitly to rewrite the committed fixture:
///
/// ```text
/// cargo test -p nmp-app-podcast regenerate_podcast_update_widget_fixture -- --ignored --nocapture
/// ```
#[test]
#[ignore = "regeneration helper; run with --ignored to rewrite the fixture"]
fn regenerate_podcast_update_widget_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/podcast_update_with_widget.json");
    let mut json = serde_json::to_string_pretty(&fixture_update()).expect("encode");
    json.push('\n');
    std::fs::write(&path, json).expect("write fixture");
    eprintln!("wrote {}", path.display());
}

// ── Chapters + transcripts cross-language fixture ─────────────────────────────
//
// `tests/fixtures/podcast_update_with_chapters.json` is a Rust-emitted
// `PodcastUpdate` whose episode carries populated `chapters` and
// `transcript_entries` — the two embedded-Vec types most likely to trigger the
// #371-class failure (a required field serialising as `null` under NaN).
//
// The Swift `PodcastUpdateChapterDecodeTests` decodes the same bytes through
// `KernelDecoding.decodePodcastUpdate` and asserts chapters + transcripts
// survive — so any Rust↔Swift schema divergence (wrong field name, missing
// CodingKeys, non-Option required field going null) fails CI instead of
// freezing the app.

/// Build the canonical chapters fixture update.
fn chapters_fixture_update() -> crate::ffi::snapshot_update::PodcastUpdate {
    use crate::ffi::projections::ChapterSummary;
    use crate::ffi::snapshot_update::PodcastUpdate;

    let ep = EpisodeSummary {
        id: "ep-ch-1".into(),
        title: "Deep Dive into Metabolic Flexibility".into(),
        artwork_url: Some("https://ex.com/ch.png".into()),
        duration_secs: Some(3600.0),
        chapters: vec![
            ChapterSummary {
                start_secs: 0.0,
                end_secs: Some(300.0),
                title: "Introduction".into(),
                image_url: None,
                url: None,
                is_ai_generated: false,
                ..ChapterSummary::default()
            },
            ChapterSummary {
                start_secs: 300.0,
                end_secs: Some(1200.0),
                title: "What is Metabolic Flexibility?".into(),
                image_url: Some("https://ex.com/ch2.png".into()),
                url: Some("https://ex.com/notes#2".into()),
                is_ai_generated: false,
                ..ChapterSummary::default()
            },
            ChapterSummary {
                start_secs: 1200.0,
                end_secs: Some(3600.0),
                title: "AI-Generated Deep Dive".into(),
                is_ai_generated: true,
                ..ChapterSummary::default()
            },
        ],
        transcript_entries: vec![
            crate::ffi::projections::TranscriptEntry {
                start_secs: 0.0,
                end_secs: Some(15.0),
                speaker: Some("Host".into()),
                text: "Welcome back to the show.".into(),
            },
            crate::ffi::projections::TranscriptEntry {
                start_secs: 15.0,
                end_secs: None,
                speaker: None,
                text: "Today we explore metabolic flexibility.".into(),
            },
        ],
        ..Default::default()
    };
    let library = vec![show("show-ch-1", "Science Podcast", 3, vec![ep])];
    let state = playing("ep-ch-1", 600.0, 3600.0);
    let widget = build_widget_snapshot(Some(&state), &library);

    PodcastUpdate {
        running: true,
        rev: 2,
        schema_version: 1,
        library,
        widget,
        ..PodcastUpdate::default()
    }
}

/// Pin the chapters + transcripts wire shape against the committed fixture.
/// The Swift `PodcastUpdateChapterDecodeTests` decodes the same bytes through
/// `KernelDecoding` so a schema drift fails CI, not the live app.
#[test]
fn podcast_update_with_chapters_matches_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/podcast_update_with_chapters.json");
    let actual = serde_json::to_string_pretty(&chapters_fixture_update()).expect("encode");

    if !path.exists() {
        // First run: generate the fixture. Commit the result.
        let mut content = actual.clone();
        content.push('\n');
        std::fs::write(&path, content).expect("write chapters fixture");
        eprintln!(
            "Chapters fixture written ({} bytes). Commit it — subsequent runs assert byte-identical.",
            actual.len()
        );
        return; // First run always green.
    }

    let expected = std::fs::read_to_string(&path).expect("read chapters fixture");
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "PodcastUpdate wire shape (chapters/transcripts) drifted from \
         tests/fixtures/podcast_update_with_chapters.json.\n\
         If intentional, regenerate:\n\
         \tcargo test -p nmp-app-podcast regenerate_chapters_fixture -- --ignored --nocapture\n\
         The Swift PodcastUpdateChapterDecodeTests decodes the same bytes through \
         KernelDecoding; keep them in sync."
    );
}

/// Regeneration helper for [`podcast_update_with_chapters_matches_fixture`].
#[test]
#[ignore = "regeneration helper; run with --ignored to rewrite the fixture"]
fn regenerate_chapters_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/podcast_update_with_chapters.json");
    let mut json = serde_json::to_string_pretty(&chapters_fixture_update()).expect("encode");
    json.push('\n');
    std::fs::write(&path, json).expect("write chapters fixture");
    eprintln!("wrote {}", path.display());
}

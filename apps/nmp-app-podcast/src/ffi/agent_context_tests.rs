use super::*;
use crate::ffi::projections::EpisodeSummary;

/// Fixed "now" for deterministic recency cutoffs (2026-01-01T00:00:00Z).
const NOW: i64 = 1_767_225_600;
const DAY: i64 = 86_400;

fn show(title: &str, episodes: Vec<EpisodeSummary>) -> PodcastSummary {
    PodcastSummary {
        id: title.into(),
        title: title.into(),
        episodes,
        ..PodcastSummary::default()
    }
}

fn ep(title: &str, published_at: i64, position: Option<f64>, played: bool) -> EpisodeSummary {
    EpisodeSummary {
        id: title.into(),
        title: title.into(),
        published_at: Some(published_at),
        playback_position_secs: position,
        played,
        ..EpisodeSummary::default()
    }
}

#[test]
fn subscriptions_sorted_case_insensitively_and_total_tracks_precap() {
    let library = vec![show("Zebra", vec![]), show("apple", vec![]), show("Mango", vec![])];
    let ctx = build_agent_context(&library, NOW);
    assert_eq!(ctx.subscriptions, vec!["apple", "Mango", "Zebra"]);
    assert_eq!(ctx.subscriptions_total, 3);
}

#[test]
fn subscriptions_capped_but_total_is_full_count() {
    let library: Vec<PodcastSummary> = (0..40)
        // Zero-pad so lexical sort == numeric order, making the cap deterministic.
        .map(|i| show(&format!("Show {i:02}"), vec![]))
        .collect();
    let ctx = build_agent_context(&library, NOW);
    assert_eq!(ctx.subscriptions.len(), cap::SUBSCRIPTIONS);
    assert_eq!(ctx.subscriptions_total, 40);
    assert_eq!(ctx.subscriptions.first().unwrap(), "Show 00");
    assert_eq!(ctx.subscriptions.last().unwrap(), "Show 29");
}

#[test]
fn in_progress_requires_position_and_excludes_played_and_archived() {
    let mut archived = ep("Archived", NOW - DAY, Some(120.0), false);
    archived.triage_decision = Some("archived".into());
    let library = vec![show(
        "Show",
        vec![
            ep("Started", NOW - DAY, Some(60.0), false),
            ep("Fresh", NOW - DAY, None, false),         // position 0 → not in-progress
            ep("Finished", NOW - DAY, Some(90.0), true), // played → excluded
            archived,                                    // archived → excluded
        ],
    )];
    let ctx = build_agent_context(&library, NOW);
    let titles: Vec<&str> = ctx.in_progress.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, vec!["Started"]);
}

#[test]
fn recent_unplayed_respects_window_and_zero_position() {
    let library = vec![show(
        "Show",
        vec![
            ep("Today", NOW - DAY, None, false),     // in window, unplayed, fresh
            ep("Old", NOW - 30 * DAY, None, false),  // outside 7-day window
            ep("Resumed", NOW - DAY, Some(30.0), false), // started → not "unplayed/fresh"
            ep("Done", NOW - DAY, None, true),       // played
        ],
    )];
    let ctx = build_agent_context(&library, NOW);
    let titles: Vec<&str> = ctx.recent_unplayed.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, vec!["Today"]);
    assert_eq!(ctx.recent_window_days, cap::RECENT_WINDOW_DAYS);
}

#[test]
fn recent_unplayed_sorted_newest_first_across_shows_then_capped() {
    // 12 fresh, in-window, unplayed episodes split across two shows. Expect
    // the 10 newest, globally sorted, regardless of which show they're in.
    let make = |prefix: &str| -> Vec<EpisodeSummary> {
        (0..6)
            .map(|i| ep(&format!("{prefix}-{i}"), NOW - DAY - i64::from(i) * 3_600, None, false))
            .collect()
    };
    let library = vec![show("A", make("a")), show("B", make("b"))];
    let ctx = build_agent_context(&library, NOW);
    assert_eq!(ctx.recent_unplayed.len(), cap::RECENT_UNPLAYED);
    // Newest two are a-0 (NOW-DAY) and b-0 (NOW-DAY) — equal timestamps keep
    // library order (show A before show B). The oldest two (a-5, b-5) drop.
    let titles: Vec<&str> = ctx.recent_unplayed.iter().map(|e| e.title.as_str()).collect();
    assert!(!titles.contains(&"a-5"));
    assert!(!titles.contains(&"b-5"));
    assert_eq!(titles[0], "a-0");
    assert_eq!(titles[1], "b-0");
}

#[test]
fn episode_rows_carry_resolved_show_title() {
    let library = vec![show("My Show", vec![ep("Ep", NOW - DAY, Some(10.0), false)])];
    let ctx = build_agent_context(&library, NOW);
    assert_eq!(ctx.in_progress[0].show_title, "My Show");
    assert_eq!(ctx.in_progress[0].title, "Ep");
}

#[test]
fn empty_library_yields_empty_context() {
    let ctx = build_agent_context(&[], NOW);
    assert!(ctx.subscriptions.is_empty());
    assert_eq!(ctx.subscriptions_total, 0);
    assert!(ctx.in_progress.is_empty());
    assert!(ctx.recent_unplayed.is_empty());
}

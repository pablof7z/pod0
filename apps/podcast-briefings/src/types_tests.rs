//! Tests for [`super::types`] — BriefingStatus, BriefingSegment, BriefingSchedule,
//! and Briefing serde + invariant coverage.
//!
//! Extracted from `types.rs` to keep that file under the 500-line hard limit.

use super::*;
use chrono::TimeZone;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 7, 0, 0).unwrap()
}

#[test]
fn status_label_matches_wire() {
    assert_eq!(BriefingStatus::Pending.label(), "pending");
    assert_eq!(BriefingStatus::Generating.label(), "generating");
    assert_eq!(BriefingStatus::Ready.label(), "ready");
    assert_eq!(BriefingStatus::Delivered.label(), "delivered");
    assert_eq!(BriefingStatus::failed("boom").label(), "failed");
}

#[test]
fn status_serde_round_trip_failed() {
    let s = BriefingStatus::failed("boom");
    let j = serde_json::to_string(&s).expect("encode");
    assert_eq!(j, r#"{"type":"failed","error":"boom"}"#);
    let d: BriefingStatus = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, s);
}

#[test]
fn segment_kind_serde_round_trip() {
    for k in [
        SegmentKind::Intro,
        SegmentKind::EpisodeSummary,
        SegmentKind::NewEpisodeAlert,
        SegmentKind::WeatherUpdate,
        SegmentKind::OutroCallToAction,
    ] {
        let j = serde_json::to_string(&k).expect("encode");
        let d: SegmentKind = serde_json::from_str(&j).expect("decode");
        assert_eq!(d, k);
    }
}

#[test]
fn segment_serde_omits_none_fields() {
    let seg = BriefingSegment::new(SegmentKind::Intro, "good morning");
    let j = serde_json::to_string(&seg).expect("encode");
    assert!(!j.contains("episode_id"));
    assert!(!j.contains("duration_hint_secs"));
    let d: BriefingSegment = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, seg);
}

#[test]
fn segment_with_episode_round_trips() {
    let seg = BriefingSegment {
        kind: SegmentKind::EpisodeSummary,
        text: "Today on Hard Fork…".into(),
        episode_id: Some("ep-42".into()),
        duration_hint_secs: Some(60.0),
    };
    let j = serde_json::to_string(&seg).expect("encode");
    let d: BriefingSegment = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, seg);
}

#[test]
fn schedule_default_is_weekdays_seven_am_disabled() {
    let s = BriefingSchedule::default();
    assert_eq!(s.time_of_day, 420);
    assert_eq!(s.days, vec![1, 2, 3, 4, 5]);
    assert!(!s.enabled);
}

#[test]
fn schedule_covers_requires_enabled() {
    let mut s = BriefingSchedule::default();
    assert!(!s.covers(3), "covers should be false when disabled");
    s.enabled = true;
    assert!(s.covers(3));
    assert!(!s.covers(0), "Sunday not in default weekday schedule");
}

#[test]
fn briefing_pending_starts_empty() {
    let b = Briefing::pending(t0(), BriefingSchedule::default());
    assert_eq!(b.status, BriefingStatus::Pending);
    assert!(b.segments.is_empty());
    assert!(b.delivered_at.is_none());
    assert_eq!(b.created_at, t0());
}

#[test]
fn briefing_serde_round_trip() {
    let b = Briefing::pending(t0(), BriefingSchedule::default());
    let j = serde_json::to_string(&b).expect("encode");
    let d: Briefing = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, b);
}

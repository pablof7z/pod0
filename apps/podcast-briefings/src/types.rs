//! Briefing domain types — the composition + status vocabulary the M9
//! composer, stitcher, and player engine all encode against.
//!
//! ## Scope (M9.A)
//!
//! The M9.A skeleton fixes the wire shape for `Briefing`, its lifecycle
//! status, the editorial `BriefingSegment` rows that make up its body,
//! and the user-configurable `BriefingSchedule`. The fuller surface
//! (attribution chips, quote splicing, target durations) from the
//! legacy Swift `Briefing/BriefingSegment.swift` lands in M9.B
//! alongside the composer; M9.A keeps the wire narrow so the FFI
//! snapshot has a contract to encode against.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// BriefingStatus — lifecycle
// ---------------------------------------------------------------------------

/// Lifecycle state of a briefing.
///
/// `Failed` carries an `error: String` payload (D6 — failures are data,
/// not exceptions across the FFI). The other variants are payload-free
/// markers projected directly from the scheduler's state transitions.
///
/// Wire form is `serde`-tagged on `"type"` (`snake_case`):
///
/// ```text
/// {"type":"pending"}
/// {"type":"generating"}
/// {"type":"ready"}
/// {"type":"delivered"}
/// {"type":"failed","error":"…"}
/// ```
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BriefingStatus {
    /// The scheduler has scheduled a briefing slot but generation has not
    /// yet begun. Default state on construction.
    Pending,
    /// Composition is in progress — the agent tool (`generate_briefing`)
    /// is assembling the segment plan; the stitcher has not yet produced
    /// stitched audio.
    Generating,
    /// Composition succeeded; `segments` is populated and ready for the
    /// player engine to render. Not yet delivered (i.e. user has not
    /// pressed play / system has not surfaced the notification).
    Ready,
    /// The briefing was delivered (user listened, or the system handed
    /// it off to CarPlay / Live Activity). The briefing remains in the
    /// scheduler's history until a fresh slot rotates it out.
    Delivered,
    /// Composition failed — the agent-tool call errored, the stitcher
    /// couldn't render audio, or the knowledge layer returned an
    /// unrecoverable error. `error` is a human-readable diagnostic.
    Failed { error: String },
}

impl BriefingStatus {
    /// `pending` — the default starting state.
    #[must_use]
    pub fn pending() -> Self {
        Self::Pending
    }

    /// `failed` with the supplied error string.
    #[must_use]
    pub fn failed(error: impl Into<String>) -> Self {
        Self::Failed {
            error: error.into(),
        }
    }

    /// Short status label (`"pending"`, `"generating"`, …) used by the
    /// snapshot projection. Centralised here so the snapshot crate
    /// doesn't need to match on the enum or re-serialise it.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Delivered => "delivered",
            Self::Failed { .. } => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// SegmentKind — editorial categorisation
// ---------------------------------------------------------------------------

/// Editorial categorisation of a [`BriefingSegment`]. Drives stitching
/// policy (intro audio asset, outro cadence) and the rail-pill icon.
///
/// Wire form is lowercase snake_case (e.g. `"new_episode_alert"`).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// "Good morning, here's your briefing for …" — always the first
    /// segment. The stitcher picks an audio bed.
    Intro,
    /// Summary of a single source episode, with attribution.
    EpisodeSummary,
    /// "New from <show> overnight: …" — alert for fresh subscriptions.
    NewEpisodeAlert,
    /// Optional weather mention (skipped when location isn't authorised).
    WeatherUpdate,
    /// Sign-off + suggested next action (open the player, listen on
    /// CarPlay, etc.).
    OutroCallToAction,
}

// ---------------------------------------------------------------------------
// BriefingSegment — single editorial unit
// ---------------------------------------------------------------------------

/// A single editorial unit inside a briefing — a TTS-narrated passage
/// with an optional source-episode citation and target duration hint.
///
/// The M9.A shape mirrors the legacy Swift `BriefingSegment` narrowed
/// to the fields the composer + stitcher both need. Attribution chips,
/// quote splicing, and per-sentence ink classification land in M9.B.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BriefingSegment {
    /// Editorial category — drives the rail-pill icon + stitching policy.
    pub kind: SegmentKind,
    /// The TTS-narrated body in plain text. Becomes the live transcript
    /// pane during playback.
    pub text: String,
    /// Source episode this segment cites, when applicable. `None` for
    /// `Intro`, `WeatherUpdate`, `OutroCallToAction`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<String>,
    /// Composer-estimated target duration in seconds (TTS + any quotes).
    /// `None` until the LLM produces a pacing estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_hint_secs: Option<f32>,
}

impl BriefingSegment {
    /// Convenience: construct a segment with no episode link / duration
    /// hint (the common case for `Intro`, `OutroCallToAction`).
    #[must_use]
    pub fn new(kind: SegmentKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            episode_id: None,
            duration_hint_secs: None,
        }
    }
}

// ---------------------------------------------------------------------------
// BriefingSchedule — user-configurable slot
// ---------------------------------------------------------------------------

/// User-configurable briefing schedule. Triggers a `Pending` slot at
/// `time_of_day` on each enabled `day` of the week.
///
/// `time_of_day` is encoded as **minutes since midnight** (0..=1440)
/// rather than a `chrono::NaiveTime` so the wire shape stays a flat
/// `u32`. Matches the legacy Swift representation that the iOS settings
/// view binds against.
///
/// `days` is a sorted-unique list of weekday indices where **0 = Sunday**
/// and 6 = Saturday (matches `Calendar.current.component(.weekday)` on
/// iOS once decremented). The scheduler accepts an unsorted list — it
/// only checks membership.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSchedule {
    /// Minutes since midnight (e.g. 420 = 07:00 local time).
    pub time_of_day: u32,
    /// 0 = Sunday, 6 = Saturday. Empty = never.
    pub days: Vec<u8>,
    /// Master switch — false suppresses generation without dropping the
    /// schedule rows (so toggling back on doesn't lose the slot config).
    pub enabled: bool,
}

impl Default for BriefingSchedule {
    fn default() -> Self {
        Self {
            time_of_day: 420, // 07:00
            days: vec![1, 2, 3, 4, 5], // Mon–Fri
            enabled: false,
        }
    }
}

impl BriefingSchedule {
    /// `true` when `day` is one of the enabled days AND `enabled` is on.
    #[must_use]
    pub fn covers(&self, day: u8) -> bool {
        self.enabled && self.days.iter().any(|d| *d == day)
    }
}

// ---------------------------------------------------------------------------
// Briefing — top-level aggregate
// ---------------------------------------------------------------------------

/// A scheduled or completed briefing. The lifecycle is driven by the
/// [`crate::scheduler::BriefingScheduler`] state machine; everything
/// here is pure data the snapshot encoder reads.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Briefing {
    /// Stable identifier — used as the player engine's session id and
    /// the persistence key.
    pub id: Uuid,
    /// Lifecycle status — see [`BriefingStatus`].
    pub status: BriefingStatus,
    /// Editorial segments, in playback order. Empty until the composer
    /// completes (`status` transitions `Generating` → `Ready`).
    pub segments: Vec<BriefingSegment>,
    /// Wall-clock instant the slot was minted (typically the moment
    /// the scheduler observed the configured time-of-day).
    pub created_at: DateTime<Utc>,
    /// Wall-clock instant the briefing was delivered. `None` until
    /// `status` transitions to `Delivered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<DateTime<Utc>>,
    /// Schedule that produced this briefing (snapshotted at creation
    /// time so the segment doesn't follow later edits).
    pub schedule: BriefingSchedule,
}

impl Briefing {
    /// Construct a fresh `Pending` briefing with no segments.
    /// `created_at` is supplied by the caller (D9 — kernel owns time).
    #[must_use]
    pub fn pending(created_at: DateTime<Utc>, schedule: BriefingSchedule) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: BriefingStatus::Pending,
            segments: Vec::new(),
            created_at,
            delivered_at: None,
            schedule,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 25, 7, 0, 0).unwrap()
    }

    #[test]
    fn status_label_matches_wire() {
        assert_eq!(BriefingStatus::Pending.label(), "pending");
        assert_eq!(
            BriefingStatus::Generating.label(),
            "generating"
        );
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
}

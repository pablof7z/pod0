use serde::{Deserialize, Serialize};

/// Snapshot of the briefing scheduler surfaced via
/// [`super::snapshot::PodcastUpdate::briefing`].
///
/// Mirrors `podcast_briefings::BriefingScheduler` state with the
/// projections the UI needs:
///
/// * `status` — the lifecycle label of the active briefing
///   (`"pending"` / `"generating"` / `"ready"` / `"delivered"` /
///   `"failed"`). The full enum lives in `podcast-briefings`; here we
///   surface it as a string so the Swift decoder doesn't need the
///   enum variant case-mapping.
/// * `is_generating` — convenience flag (`status == "generating"`)
///   the iOS view binds against to render a spinner without
///   re-deriving from the label.
/// * `segment_count` — number of editorial segments produced (0
///   until `status == "ready"`).
/// * `segments` — the editorial segments themselves, projected to a
///   narrow Codable shape (`kind`, `text`, attribution titles).
///   Empty until the composer completes.
/// * `last_generated_at` — Unix seconds the most recent briefing
///   was composed/delivered. `None` until the first briefing
///   finishes.
/// * `next_scheduled_minutes` — minutes until the next scheduled
///   briefing today, when the scheduler has an active schedule that
///   covers today and the slot hasn't passed yet.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSnapshot {
    /// One of `"pending"`, `"generating"`, `"ready"`, `"delivered"`,
    /// `"failed"` — matches `podcast_briefings::BriefingStatus::label`.
    pub status: String,
    /// `true` while the briefing is being composed. Convenience flag
    /// equivalent to `status == "generating"`; surfaced separately
    /// so the iOS view doesn't reach into the label.
    pub is_generating: bool,
    /// Number of editorial segments in the active briefing. Zero
    /// until the composer completes.
    pub segment_count: usize,
    /// Editorial segments in playback order. Empty until the composer
    /// completes (`status == "ready"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<BriefingSegmentSummary>,
    /// Unix seconds the most recent briefing was composed/delivered.
    /// `None` until the first briefing finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_generated_at: Option<i64>,
    /// Minutes until the next scheduled briefing slot on the current
    /// calendar day. `None` when no schedule is active, when today
    /// isn't covered, or when the slot has already passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_scheduled_minutes: Option<u32>,
}

/// One row in [`BriefingSnapshot::segments`].
///
/// Narrow Codable mirror of `podcast_briefings::BriefingSegment` — the
/// fields the iOS card view actually renders. The richer internal
/// segment shape (`episode_id`, `duration_hint_secs`) stays in
/// `podcast-briefings`; the projection surfaces titles instead of ids
/// so the view doesn't need to look up the library to render
/// attribution.
///
/// `kind` is the snake_case label from `podcast_briefings::SegmentKind`
/// (`"intro"`, `"episode_summary"`, `"new_episode_alert"`,
/// `"weather_update"`, `"outro_call_to_action"`) so the Swift decoder
/// can switch on a plain string.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct BriefingSegmentSummary {
    /// Editorial category — see struct-level doc.
    pub kind: String,
    /// TTS-narrated body text, plain.
    pub text: String,
    /// Source podcast title for attribution; `None` for intro /
    /// outro / weather.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_title: Option<String>,
    /// Source episode title for attribution; `None` for intro /
    /// outro / weather.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_title: Option<String>,
}

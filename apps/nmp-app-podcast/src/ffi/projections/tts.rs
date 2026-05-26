use serde::{Deserialize, Serialize};

/// One row in the agent-generated TTS episode list surfaced via
/// [`super::snapshot::PodcastUpdate::tts_episodes`].
///
/// These are not "real" podcast episodes — they live entirely in
/// kernel-side memory on the [`super::handle::PodcastHandle`], not in
/// [`crate::store::PodcastStore`], because they don't have a feed, an
/// enclosure URL, or any of the other RSS-derived fields the
/// [`EpisodeSummary`] projection carries. The script string is the
/// text that the iOS voice executor will speak when the user taps
/// "play"; the kernel mints it and never has to re-derive it.
///
/// `status` is a string discriminator (`"generating_script"` |
/// `"ready"` | `"played"`) rather than a typed enum so the Swift
/// `Codable` decoder doesn't need a case-mapping for what is purely a
/// display chip in the list.
///
/// `voice_id` is `Option` because the M0 stub generator does not pick
/// a voice — the executor falls back to its currently configured one.
/// A future LLM-script generator may choose a voice per-episode.
///
/// `Eq` is intentionally not derived because `duration_estimate_secs`
/// is `f64`; partial equality (`PartialEq`) is sufficient for snapshot
/// round-trip tests where the value goes through serde without
/// arithmetic.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TtsEpisodeSummary {
    /// Stable UUID minted by the kernel on `generate`. Rendered as
    /// the canonical hyphenated string for Swift `Identifiable`.
    pub id: String,
    pub title: String,
    /// The plain-text script that the voice capability will speak.
    /// Surfaced to the iOS list so the user can preview before
    /// tapping play (truncated by the UI as needed).
    pub script: String,
    /// Best-effort duration estimate. Computed by the kernel from the
    /// requested `length_minutes` (so generating a "5 minute" episode
    /// yields `300.0` seconds even though the placeholder script
    /// itself is much shorter). The follow-up LLM generator will
    /// replace this with an actual word-count-based estimate.
    pub duration_estimate_secs: f64,
    /// Unix seconds at the moment `generate` was dispatched.
    pub created_at: i64,
    /// One of `"generating_script"`, `"ready"`, `"played"`. The M0
    /// stub generator emits `"ready"` immediately; the future LLM
    /// generator will surface `"generating_script"` while the script
    /// is being synthesised.
    pub status: String,
    /// Optional voice id (provider-specific opaque string). `None`
    /// means "use the executor's currently configured voice."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
}

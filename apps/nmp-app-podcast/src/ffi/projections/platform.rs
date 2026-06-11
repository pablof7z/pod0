use serde::{Deserialize, Serialize};

/// Narrow projection consumed by the M11 platform-integration
/// executors (widget extension, Live Activity, Handoff,
/// Siri shortcuts). It is **not** a superset of `now_playing` —
/// the shape is intentionally lossy so the platform extensions
/// don't have to depend on the full player + downloads schemas.
///
/// Per D7 the kernel chooses what to surface; if a field is
/// missing here, the widget renders its empty state. The Rust
/// projection layer builds this from `PlayerState` +
/// `DownloadQueue` + the unplayed-episode count on each tick;
/// the iOS shell serializes it into the App Group `UserDefaults`
/// key the widget extension reads (see
/// `PlatformCapability.writeWidgetSnapshot(_:)`).
///
/// `position_fraction` is pre-computed (`position_secs /
/// duration_secs`, clamped to `0.0..=1.0`) so the widget can
/// render a progress ring without doing math on possibly-zero
/// duration; `0.0` is the safe default both for "no episode"
/// and "duration unknown".
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WidgetSnapshot {
    /// Title of the active episode, when one is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_episode_title: Option<String>,
    /// Title of the podcast/show the active episode belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_podcast_title: Option<String>,
    /// Artwork URL (episode-level preferred, falls back to show).
    ///
    /// A *URL*, not pixel data — the widget extension resolves it with
    /// `AsyncImage`, which the WidgetKit process is allowed to fetch via
    /// its own `URLSession` (no separate network entitlement is required
    /// for the extension's image loads). The kernel never ships artwork
    /// bytes; it ships the same URL the in-app player resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_artwork_url: Option<String>,
    /// Title of the chapter active at the current playhead, when the
    /// episode has navigable chapters. The widget's medium layout
    /// prefers this over the show title (it's the more specific
    /// "where am I right now" signal). `None` for chapter-less episodes —
    /// the widget then falls back to the podcast title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing_chapter_title: Option<String>,
    /// `true` while playback is engaged (the player's `is_playing`).
    pub is_playing: bool,
    /// Pre-computed progress fraction `0.0..=1.0`; the widget renders
    /// this as a ring/bar without re-deriving from secs+duration.
    pub position_fraction: f32,
    /// Current playhead in seconds. Surfaced alongside the pre-computed
    /// `position_fraction` so the widget can render the "−MM:SS
    /// remaining" label without re-deriving secs from the fraction.
    /// `0.0` when no episode is loaded.
    pub position_secs: f64,
    /// Track duration in seconds; `0.0` until the capability reports it.
    /// Paired with `position_secs` so the widget's remaining-time label
    /// renders the exact value the in-app player shows.
    pub duration_secs: f64,
    /// Number of unplayed episodes across all subscribed shows;
    /// drives the badge / "X to listen" line in the widget.
    pub unplayed_count: usize,
}

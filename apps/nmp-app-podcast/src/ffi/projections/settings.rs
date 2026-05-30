use serde::{Deserialize, Serialize};

fn default_skip_forward_secs() -> f64 { 30.0 }
fn default_skip_backward_secs() -> f64 { 15.0 }
fn default_one() -> f64 { 1.0 }
fn default_true() -> bool { true }
fn default_skip_forward_action() -> String { "skipForward".to_owned() }
fn default_clip_now_action() -> String { "clipNow".to_owned() }

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Replaces the legacy in-memory `Settings` compat shim. The kernel
/// authoritative source is [`crate::store::PodcastStore`] accessors.
///
/// `Default` produces the fresh-install state so the snapshot builder can
/// always emit a `SettingsSnapshot` regardless of store-lock acquisition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SettingsSnapshot {
    /// Whether the user has finished the iOS onboarding flow.
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// When `true`, the player actor seeks past each ad segment.
    #[serde(default)]
    pub auto_skip_ads_enabled: bool,
    /// When `true`, the kernel auto-advances to the next queued episode
    /// on `ItemEnd`. Default `true`.
    #[serde(default = "default_true")]
    pub auto_play_next: bool,
    /// When `true`, the kernel marks the episode listened on `ItemEnd`.
    /// Default `true`.
    #[serde(default = "default_true")]
    pub auto_mark_played_at_end: bool,
    /// Raw action string for headphone double-tap gesture. Default `"skip_forward"`.
    #[serde(default = "default_skip_forward_action")]
    pub headphone_double_tap_action: String,
    /// Raw action string for headphone triple-tap gesture. Default `"clip_now"`.
    #[serde(default = "default_clip_now_action")]
    pub headphone_triple_tap_action: String,
    /// Skip-forward interval in seconds. Default 30.0.
    #[serde(default = "default_skip_forward_secs")]
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds. Default 15.0.
    #[serde(default = "default_skip_backward_secs")]
    pub skip_backward_secs: f64,
    /// Default playback rate. Default 1.0; range [0.5, 3.0].
    #[serde(default = "default_one")]
    pub default_playback_rate: f64,
    /// When `true`, downloaded files are deleted after the episode is marked played.
    #[serde(default)]
    pub auto_delete_downloads_after_played: bool,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            has_completed_onboarding: false,
            auto_skip_ads_enabled: false,
            auto_play_next: true,
            auto_mark_played_at_end: true,
            headphone_double_tap_action: "skipForward".to_owned(),
            headphone_triple_tap_action: "clipNow".to_owned(),
            skip_forward_secs: 30.0,
            skip_backward_secs: 15.0,
            default_playback_rate: 1.0,
            auto_delete_downloads_after_played: false,
        }
    }
}

impl SettingsSnapshot {
    /// Returns true when the snapshot equals `Default::default()`. Used as
    /// the `skip_serializing_if` guard on
    /// [`super::snapshot::PodcastUpdate::settings`] so the empty-state
    /// snapshot stays byte-identical to the legacy stub (D6).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

use serde::{Deserialize, Serialize};

fn default_skip_forward_secs() -> f64 { 30.0 }
fn default_skip_backward_secs() -> f64 { 15.0 }

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Narrow on purpose: the iOS shell only needs a handful of bools / floats
/// to gate UI (onboarding, ads, skip intervals). Replaces the legacy
/// in-memory `Settings` compat shim. The kernel authoritative source is
/// [`crate::store::PodcastStore`] accessors.
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
    /// Skip-forward interval in seconds. Default 30.0.
    #[serde(default = "default_skip_forward_secs")]
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds. Default 15.0.
    #[serde(default = "default_skip_backward_secs")]
    pub skip_backward_secs: f64,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            has_completed_onboarding: false,
            auto_skip_ads_enabled: false,
            skip_forward_secs: 30.0,
            skip_backward_secs: 15.0,
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

use serde::{Deserialize, Serialize};

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Narrow on purpose: the iOS shell only needs a handful of bools / strings
/// from this struct to gate UI (onboarding flow, manual-credentials banners,
/// …). Replaces the legacy in-memory `Settings` compat shim. The kernel
/// authoritative source is [`crate::store::PodcastStore::has_completed_onboarding`].
///
/// `Default` produces the fresh-install state (`has_completed_onboarding =
/// false`) so the snapshot builder can always emit a `SettingsSnapshot`
/// regardless of store-lock acquisition.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SettingsSnapshot {
    /// Whether the user has finished the iOS onboarding flow. iOS reads
    /// this from the `settings` snapshot to decide whether to present
    /// `OnboardingView`. Mutated via the `podcast.update_settings` action.
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// When `true`, the player actor seeks past each ad segment in the
    /// currently-loaded episode. Mirrored from
    /// `PodcastStore::auto_skip_ads_enabled`. Defaults to `false`.
    #[serde(default)]
    pub auto_skip_ads_enabled: bool,
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

use crate::app::AppState;
use crate::runtime::{AppRuntime, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingItem {
    AutoPlayNext,
    AutoMarkPlayedAtEnd,
    AutoSkipAds,
    AutoDeleteDownloads,
    NotifyOnNewEpisodes,
    AutoIngestTranscripts,
    AutoFallbackToScribe,
    NostrEnabled,
    DefaultPlaybackRate,
}

pub const SETTINGS_ITEMS: [SettingItem; 9] = [
    SettingItem::AutoPlayNext,
    SettingItem::AutoMarkPlayedAtEnd,
    SettingItem::AutoSkipAds,
    SettingItem::AutoDeleteDownloads,
    SettingItem::NotifyOnNewEpisodes,
    SettingItem::AutoIngestTranscripts,
    SettingItem::AutoFallbackToScribe,
    SettingItem::NostrEnabled,
    SettingItem::DefaultPlaybackRate,
];

impl SettingItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::AutoPlayNext => "Auto-play next",
            Self::AutoMarkPlayedAtEnd => "Mark played at end",
            Self::AutoSkipAds => "Auto-skip ads",
            Self::AutoDeleteDownloads => "Delete downloads after played",
            Self::NotifyOnNewEpisodes => "Notify on new episodes",
            Self::AutoIngestTranscripts => "Auto-ingest publisher transcripts",
            Self::AutoFallbackToScribe => "Fallback to Scribe",
            Self::NostrEnabled => "Nostr features",
            Self::DefaultPlaybackRate => "Default playback rate",
        }
    }

    pub fn value(self, state: &AppState) -> String {
        match self {
            Self::AutoPlayNext => bool_label(state.settings.auto_play_next),
            Self::AutoMarkPlayedAtEnd => bool_label(state.settings.auto_mark_played_at_end),
            Self::AutoSkipAds => bool_label(state.settings.auto_skip_ads_enabled),
            Self::AutoDeleteDownloads => {
                bool_label(state.settings.auto_delete_downloads_after_played)
            }
            Self::NotifyOnNewEpisodes => bool_label(state.settings.notify_on_new_episodes),
            Self::AutoIngestTranscripts => {
                bool_label(state.settings.auto_ingest_publisher_transcripts)
            }
            Self::AutoFallbackToScribe => bool_label(state.settings.auto_fallback_to_scribe),
            Self::NostrEnabled => bool_label(state.settings.nostr_enabled),
            Self::DefaultPlaybackRate => format!("{:.2}x", state.settings.default_playback_rate),
        }
    }

    pub fn activate(self, state: &AppState, runtime: &AppRuntime) -> Result<String> {
        match self {
            Self::AutoPlayNext => runtime.set_auto_play_next(!state.settings.auto_play_next),
            Self::AutoMarkPlayedAtEnd => {
                runtime.set_auto_mark_played_at_end(!state.settings.auto_mark_played_at_end)
            }
            Self::AutoSkipAds => runtime.set_auto_skip_ads(!state.settings.auto_skip_ads_enabled),
            Self::AutoDeleteDownloads => runtime.set_auto_delete_downloads_after_played(
                !state.settings.auto_delete_downloads_after_played,
            ),
            Self::NotifyOnNewEpisodes => {
                runtime.set_notify_on_new_episodes(!state.settings.notify_on_new_episodes)
            }
            Self::AutoIngestTranscripts => runtime.set_auto_ingest_publisher_transcripts(
                !state.settings.auto_ingest_publisher_transcripts,
            ),
            Self::AutoFallbackToScribe => {
                runtime.set_auto_fallback_to_scribe(!state.settings.auto_fallback_to_scribe)
            }
            Self::NostrEnabled => runtime.set_nostr_enabled(!state.settings.nostr_enabled),
            Self::DefaultPlaybackRate => {
                runtime.set_default_playback_rate(next_rate(state.settings.default_playback_rate))
            }
        }
    }
}

fn bool_label(value: bool) -> String {
    if value {
        "on".to_string()
    } else {
        "off".to_string()
    }
}

fn next_rate(current: f64) -> f64 {
    const RATES: [f64; 7] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5];
    RATES
        .into_iter()
        .find(|rate| *rate > current + 0.01)
        .unwrap_or(RATES[0])
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::podcast::PodcastId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AutoDownloadMode {
    Off,
    LatestN { count: u32 },
    AllNew,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoDownloadPolicy {
    #[serde(flatten)]
    pub mode: AutoDownloadMode,
    pub wifi_only: bool,
}

impl AutoDownloadPolicy {
    pub fn new(mode: AutoDownloadMode, wifi_only: bool) -> Self {
        Self { mode, wifi_only }
    }

    pub fn default_policy() -> Self {
        Self {
            mode: AutoDownloadMode::AllNew,
            wifi_only: true,
        }
    }
}

impl Default for AutoDownloadPolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PodcastSubscription {
    pub podcast_id: PodcastId,
    pub subscribed_at: DateTime<Utc>,
    pub auto_download: AutoDownloadPolicy,
    pub notifications_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_playback_rate: Option<f64>,
}

impl PodcastSubscription {
    pub fn new(podcast_id: PodcastId) -> Self {
        Self {
            podcast_id,
            subscribed_at: Utc::now(),
            auto_download: AutoDownloadPolicy::default_policy(),
            notifications_enabled: true,
            default_playback_rate: None,
        }
    }

    pub fn id(&self) -> PodcastId {
        self.podcast_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn policy_round_trip() {
        let value = AutoDownloadPolicy {
            mode: AutoDownloadMode::LatestN { count: 5 },
            wifi_only: false,
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: AutoDownloadPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn subscription_round_trip() {
        let value = PodcastSubscription::new(PodcastId::new(Uuid::nil()));
        let json = serde_json::to_string(&value).unwrap();
        let back: PodcastSubscription = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum SettingMutation {
    NewEpisodeNotifications {
        enabled: bool,
    },
    SubscriptionNotifications {
        podcast_id: String,
        enabled: bool,
    },
    SubscriptionAutoDownload {
        podcast_id: String,
        mode: AutoDownloadModeDto,
        wifi_only: bool,
    },
    SubscriptionTranscriptPolicy {
        podcast_id: String,
        policy: TranscriptPolicyDto,
    },
    PlaybackRate {
        permille: u16,
    },
    PlaybackPreferences {
        auto_mark_played_at_natural_end: bool,
        auto_play_next: bool,
        auto_skip_ads: bool,
    },
    Recall {
        stored_embedding_model_id: String,
        reranker_enabled: bool,
    },
    Workflow {
        value: WorkflowSettingsDto,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AutoDownloadModeDto {
    Off,
    Latest { count: u16 },
    AllNew,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptPolicyDto {
    Automatic,
    WhenPlayed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkflowSettingsDto {
    pub transcript_provider: String,
    pub eleven_labs_model: String,
    pub assembly_ai_model: String,
    pub open_router_model: String,
    pub auto_publisher_transcripts: bool,
    pub auto_provider_transcripts: bool,
    pub chapter_model: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SettingsDto {
    pub new_episode_notifications: RevisionedBoolDto,
    pub subscriptions: Vec<SubscriptionSettingsDto>,
    pub subscription_offset: u32,
    pub subscription_limit: u16,
    pub subscription_total: usize,
    pub subscriptions_has_more: bool,
    pub playback: PlaybackSettingsDto,
    pub recall: RecallSettingsDto,
    pub workflow: Option<WorkflowSettingsStateDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RevisionedBoolDto {
    pub enabled: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubscriptionSettingsDto {
    pub podcast_id: String,
    pub notifications_enabled: bool,
    pub auto_download: AutoDownloadSettingsDto,
    pub transcript_start_policy: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutoDownloadSettingsDto {
    pub mode: String,
    pub latest_count: Option<u16>,
    pub wifi_only: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlaybackSettingsDto {
    pub rate_permille: u16,
    pub auto_mark_played_at_natural_end: bool,
    pub auto_play_next: bool,
    pub auto_skip_ads: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecallSettingsDto {
    pub revision: u64,
    pub stored_embedding_model_id: String,
    pub reranker_enabled: bool,
    pub embedding_provider: String,
    pub embedding_model: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowSettingsStateDto {
    pub revision: u64,
    #[serde(flatten)]
    pub value: WorkflowSettingsDto,
}

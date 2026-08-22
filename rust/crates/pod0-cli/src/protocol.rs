mod settings;

use serde::{Deserialize, Serialize};

pub use settings::*;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct CliRequest {
    #[serde(default = "protocol_version", rename = "v")]
    pub version: u32,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub command: CliCommand,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CliCommand {
    Status,
    Help,
    CreateStore {
        path: String,
    },
    OpenStore {
        path: String,
    },
    SubscribeFeed {
        feed_url: String,
        #[serde(default)]
        offset: u32,
        #[serde(default = "default_page_limit")]
        limit: u16,
    },
    SearchPodcasts {
        term: String,
        #[serde(default = "default_search_limit")]
        limit: u16,
    },
    SettingsGet {
        #[serde(default)]
        offset: u32,
        #[serde(default = "default_page_limit")]
        limit: u16,
    },
    SettingsSet {
        setting: SettingMutation,
        #[serde(default)]
        offset: u32,
        #[serde(default = "default_page_limit")]
        limit: u16,
    },
    AskAgent {
        input: String,
        #[serde(default)]
        conversation_id: Option<String>,
        #[serde(default)]
        provider: Option<AgentProvider>,
        #[serde(default)]
        model: Option<String>,
    },
    HostDrain {
        #[serde(default = "default_host_limit")]
        limit: u16,
    },
    Library {
        #[serde(default)]
        offset: u32,
        #[serde(default = "default_page_limit")]
        limit: u16,
    },
    Play {
        episode_id: String,
    },
    Pause,
    Resume,
    Exit,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProvider {
    OpenAiCompatible,
    Ollama,
}

#[derive(Clone, Debug, Serialize)]
pub struct CliResponse {
    #[serde(rename = "v")]
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CliError>,
}

impl CliResponse {
    pub(crate) fn success(request_id: Option<String>, result: ResponseData) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub(crate) fn failure(request_id: Option<String>, error: CliError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseData {
    Status {
        facade_contract_version: u32,
        store_path: Option<String>,
        capabilities: CapabilityStatus,
    },
    Help {
        commands: Vec<&'static str>,
    },
    Store {
        path: String,
        created: bool,
    },
    FeedSubscription {
        command_id: String,
        operation: OperationDto,
        offset: u32,
        limit: u16,
        has_more: bool,
        podcast_total: usize,
        subscription_total: usize,
        episode_total: usize,
        podcasts: Vec<FeedPodcastDto>,
        episodes: Vec<FeedEpisodeDto>,
    },
    Settings {
        value: Box<SettingsDto>,
        operation: Option<OperationDto>,
    },
    Agent {
        conversation_id: String,
        turn_id: String,
        stage: String,
        messages: Vec<MessageDto>,
        safe_failure: Option<String>,
    },
    HostDrain {
        pending: Vec<PendingHostWorkDto>,
    },
    Library {
        offset: u32,
        limit: u16,
        has_more: bool,
        podcast_total: usize,
        subscription_total: usize,
        episode_total: usize,
        podcasts: Vec<FeedPodcastDto>,
        episodes: Vec<FeedEpisodeDto>,
    },
    Playback {
        command_id: String,
        operation: OperationDto,
    },
    PodcastSearch {
        results: Vec<PodcastSearchResultDto>,
    },
    Exit,
}

#[derive(Clone, Debug, Serialize)]
pub struct CapabilityStatus {
    pub feed_http: bool,
    pub library_http: bool,
    pub openai_compatible: bool,
    pub ollama: bool,
    pub agent_tools: bool,
    pub agent_capability_execution: bool,
    pub audio_playback: bool,
    pub clip_media: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct OperationDto {
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FeedPodcastDto {
    pub podcast_id: String,
    pub title: String,
    pub author: String,
    pub feed_url: Option<String>,
    pub subscribed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FeedEpisodeDto {
    pub episode_id: String,
    pub podcast_id: String,
    pub publisher_guid: String,
    pub title: String,
    pub published_at_milliseconds: i64,
    pub enclosure_url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageDto {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PendingHostWorkDto {
    pub kind: String,
    pub not_before_milliseconds: Option<i64>,
    pub deadline_milliseconds: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PodcastSearchResultDto {
    pub itunes_id: u64,
    pub title: String,
    pub author: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CliError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl CliError {
    pub(crate) fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            retryable,
        }
    }
}

const fn protocol_version() -> u32 {
    PROTOCOL_VERSION
}

const fn default_host_limit() -> u16 {
    32
}

const fn default_search_limit() -> u16 {
    25
}

const fn default_page_limit() -> u16 {
    100
}

pub(crate) fn bounded_page_limit(value: u16) -> u16 {
    value.clamp(1, pod0_facade::MAX_PROJECTION_ITEMS)
}

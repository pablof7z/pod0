use serde::Deserialize;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use pod0_domain::PlaybackRatePermille;

use crate::StorageError;

#[derive(Deserialize)]
pub(crate) struct RawAppState {
    #[serde(rename = "persistenceGeneration", default)]
    pub(crate) generation: u64,
    #[serde(default)]
    pub(crate) podcasts: Option<Vec<RawPodcast>>,
    #[serde(default)]
    pub(crate) subscriptions: Vec<RawSubscription>,
    #[serde(default)]
    pub(crate) episodes: Vec<Value>,
    #[serde(default)]
    pub(crate) settings: RawSettings,
    #[serde(rename = "lastPlayedEpisodeID")]
    pub(crate) last_played_episode_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawPodcast {
    pub(crate) id: String,
    pub(crate) kind: Option<String>,
    #[serde(rename = "feedURL")]
    pub(crate) feed_url: Option<String>,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) author: String,
    #[serde(rename = "imageURL")]
    pub(crate) image_url: Option<String>,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) language: Option<String>,
    #[serde(default)]
    pub(crate) categories: Vec<String>,
    #[serde(rename = "discoveredAt")]
    pub(crate) discovered_at: Option<RawTimestamp>,
    #[serde(rename = "titleIsPlaceholder", default)]
    pub(crate) title_is_placeholder: bool,
    #[serde(rename = "lastRefreshedAt")]
    pub(crate) last_refreshed_at: Option<RawTimestamp>,
    pub(crate) etag: Option<String>,
    #[serde(rename = "lastModified")]
    pub(crate) last_modified: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawSubscription {
    #[serde(rename = "podcastID")]
    pub(crate) podcast_id: Option<String>,
    pub(crate) id: Option<String>,
    #[serde(rename = "subscribedAt")]
    pub(crate) subscribed_at: Option<RawTimestamp>,
    #[serde(rename = "autoDownload")]
    pub(crate) auto_download: Option<RawAutoDownload>,
    #[serde(rename = "notificationsEnabled")]
    pub(crate) notifications_enabled: Option<bool>,
    #[serde(rename = "defaultPlaybackRate")]
    pub(crate) default_playback_rate: Option<f64>,
    #[serde(rename = "feedURL")]
    pub(crate) feed_url: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    #[serde(rename = "imageURL")]
    pub(crate) image_url: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) categories: Option<Vec<String>>,
    #[serde(rename = "lastRefreshedAt")]
    pub(crate) last_refreshed_at: Option<RawTimestamp>,
    pub(crate) etag: Option<String>,
    #[serde(rename = "lastModified")]
    pub(crate) last_modified: Option<String>,
    #[serde(rename = "isAgentGenerated")]
    pub(crate) is_agent_generated: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct RawAutoDownload {
    pub(crate) mode: Value,
    #[serde(rename = "wifiOnly", default = "default_true")]
    pub(crate) wifi_only: bool,
}

#[derive(Default, Deserialize)]
pub(crate) struct RawSettings {
    #[serde(rename = "defaultPlaybackRate")]
    pub(crate) default_playback_rate: Option<f64>,
    #[serde(rename = "autoMarkPlayedAtEnd")]
    pub(crate) auto_mark_played_at_end: Option<bool>,
    #[serde(rename = "autoPlayNext")]
    pub(crate) auto_play_next: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct RawEpisode {
    pub(crate) id: String,
    #[serde(rename = "podcastID")]
    pub(crate) podcast_id: Option<String>,
    #[serde(rename = "subscriptionID")]
    pub(crate) legacy_subscription_id: Option<String>,
    pub(crate) guid: String,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(rename = "pubDate")]
    pub(crate) published_at: Option<RawTimestamp>,
    pub(crate) duration: Option<f64>,
    #[serde(rename = "enclosureURL")]
    pub(crate) enclosure_url: String,
    #[serde(rename = "enclosureMimeType")]
    pub(crate) enclosure_mime_type: Option<String>,
    #[serde(rename = "imageURL")]
    pub(crate) image_url: Option<String>,
    #[serde(rename = "playbackPosition", default)]
    pub(crate) playback_position: f64,
    #[serde(default)]
    pub(crate) played: bool,
    #[serde(rename = "isStarred", default)]
    pub(crate) is_starred: bool,
    #[serde(rename = "downloadState")]
    pub(crate) download_state: Option<Value>,
    #[serde(rename = "transcriptState")]
    pub(crate) transcript_state: Option<Value>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawTimestamp {
    Iso8601(String),
    SwiftReferenceSeconds(f64),
}

pub(crate) fn timestamp_milliseconds(
    value: Option<&RawTimestamp>,
    entity: &'static str,
    index: u32,
) -> Result<i64, StorageError> {
    let Some(value) = value else { return Ok(0) };
    match value {
        RawTimestamp::Iso8601(value) => {
            let parsed = OffsetDateTime::parse(value, &Rfc3339).map_err(|_| {
                StorageError::InvalidLegacyRecord {
                    entity,
                    index,
                    detail: "timestamp is not ISO-8601",
                }
            })?;
            i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).map_err(|_| {
                StorageError::InvalidLegacyRecord {
                    entity,
                    index,
                    detail: "timestamp is outside supported range",
                }
            })
        }
        RawTimestamp::SwiftReferenceSeconds(value) => {
            finite_milliseconds(*value + 978_307_200.0, entity, index)
        }
    }
}

pub(crate) fn finite_milliseconds(
    seconds: f64,
    entity: &'static str,
    index: u32,
) -> Result<i64, StorageError> {
    let milliseconds = seconds * 1_000.0;
    if !milliseconds.is_finite() || milliseconds < 0.0 || milliseconds > i64::MAX as f64 {
        return Err(StorageError::InvalidLegacyRecord {
            entity,
            index,
            detail: "duration or position is outside supported range",
        });
    }
    Ok(milliseconds.round() as i64)
}

pub(crate) fn uuid_bytes(
    value: &str,
    entity: &'static str,
    index: u32,
) -> Result<[u8; 16], StorageError> {
    let compact: String = value
        .chars()
        .filter(|character| *character != '-')
        .collect();
    if compact.len() != 32 {
        return Err(invalid_uuid(entity, index));
    }
    let mut output = [0_u8; 16];
    for (offset, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&compact[offset * 2..offset * 2 + 2], 16)
            .map_err(|_| invalid_uuid(entity, index))?;
    }
    Ok(output)
}

fn invalid_uuid(entity: &'static str, index: u32) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity,
        index,
        detail: "identity is not a UUID",
    }
}

pub(crate) fn enum_variant(value: &Value) -> Option<(&str, &Value)> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object
        .iter()
        .next()
        .map(|(key, value)| (key.as_str(), value))
}

pub(crate) const fn default_true() -> bool {
    true
}

pub(crate) fn unknown_wire_code(value: &str) -> u32 {
    value
        .as_bytes()
        .iter()
        .fold(2_166_136_261_u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(16_777_619)
        })
}

pub(crate) fn playback_rate(
    value: f64,
    entity: &'static str,
    index: u32,
) -> Result<PlaybackRatePermille, StorageError> {
    let scaled = value * 1_000.0;
    if !scaled.is_finite() || !(500.0..=3_000.0).contains(&scaled) {
        return Err(StorageError::InvalidLegacyRecord {
            entity,
            index,
            detail: "playback rate is outside 0.5x through 3.0x",
        });
    }
    Ok(PlaybackRatePermille {
        value: scaled.round() as u16,
    })
}

pub(crate) fn checked_count(value: usize, entity: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::ImportLimitExceeded { entity })
}

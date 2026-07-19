use serde::Deserialize;

use crate::legacy_format::RawTimestamp;

#[derive(Deserialize)]
pub(crate) struct RawClip {
    pub(crate) id: String,
    #[serde(rename = "episodeID")]
    pub(crate) episode_id: String,
    #[serde(rename = "subscriptionID")]
    pub(crate) podcast_id: String,
    #[serde(rename = "startMs")]
    pub(crate) start_milliseconds: i64,
    #[serde(rename = "endMs")]
    pub(crate) end_milliseconds: i64,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<RawTimestamp>,
    pub(crate) caption: Option<String>,
    #[serde(rename = "speakerID")]
    pub(crate) speaker_id: Option<String>,
    #[serde(rename = "transcriptText", default)]
    pub(crate) frozen_transcript_text: String,
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) deleted: bool,
}

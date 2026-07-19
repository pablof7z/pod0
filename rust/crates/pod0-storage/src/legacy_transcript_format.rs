use serde::Deserialize;

use crate::legacy_format::RawTimestamp;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTranscript {
    pub(crate) id: String,
    #[serde(rename = "episodeID")]
    pub(crate) episode_id: String,
    pub(crate) language: String,
    pub(crate) source: String,
    pub(crate) segments: Vec<RawTranscriptSegment>,
    pub(crate) speakers: Vec<RawTranscriptSpeaker>,
    #[serde(rename = "generatedAt")]
    pub(crate) generated_at: RawTimestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTranscriptSegment {
    pub(crate) id: String,
    pub(crate) start: f64,
    pub(crate) end: f64,
    #[serde(rename = "speakerID")]
    pub(crate) speaker_id: Option<String>,
    pub(crate) text: String,
    pub(crate) words: Option<Vec<RawTranscriptWord>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTranscriptWord {
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTranscriptSpeaker {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(rename = "displayName")]
    pub(crate) display_name: Option<String>,
}

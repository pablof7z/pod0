use serde::Deserialize;

use crate::legacy_format::RawTimestamp;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawChapter {
    pub(crate) id: Option<String>,
    #[serde(rename = "startTime")]
    pub(crate) start_time: f64,
    #[serde(rename = "endTime")]
    pub(crate) end_time: Option<f64>,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(rename = "imageURL")]
    pub(crate) image_url: Option<String>,
    #[serde(rename = "linkURL")]
    pub(crate) link_url: Option<String>,
    #[serde(rename = "includeInTableOfContents", default = "default_true")]
    pub(crate) include_in_table_of_contents: bool,
    #[serde(rename = "isAIGenerated", default)]
    pub(crate) is_ai_generated: bool,
    pub(crate) summary: Option<String>,
    #[serde(rename = "sourceEpisodeID")]
    pub(crate) source_episode_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawAdSpan {
    pub(crate) id: Option<String>,
    pub(crate) start: f64,
    pub(crate) end: f64,
    #[serde(default = "default_ad_kind")]
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawChapterEpisode {
    pub(crate) id: String,
    #[serde(rename = "podcastID")]
    pub(crate) podcast_id: Option<String>,
    #[serde(rename = "subscriptionID")]
    pub(crate) legacy_subscription_id: Option<String>,
    #[serde(rename = "pubDate")]
    pub(crate) published_at: Option<RawTimestamp>,
    pub(crate) duration: Option<f64>,
    pub(crate) chapters: Option<Vec<RawChapter>>,
    #[serde(rename = "adSegments")]
    pub(crate) ad_spans: Option<Vec<RawAdSpan>>,
    #[serde(rename = "generationSource")]
    pub(crate) generation_source: Option<RawGenerationSource>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawGenerationSource {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(rename = "conversationID")]
    pub(crate) conversation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawChapterAttempt {
    #[serde(rename = "episodeID")]
    pub(crate) episode_id: String,
    #[serde(rename = "inputVersion")]
    pub(crate) input_version: String,
    #[serde(rename = "leaseToken")]
    pub(crate) lease_token: String,
    pub(crate) output: RawChapterOutput,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawChapterOutput {
    pub(crate) chapters: Vec<RawChapter>,
    pub(crate) ads: Vec<RawAdSpan>,
    #[serde(rename = "chapterOrigin")]
    pub(crate) chapter_origin: String,
}

const fn default_true() -> bool {
    true
}

fn default_ad_kind() -> String {
    "midroll".to_owned()
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::types::ad_segment::AdSegment;
use crate::types::chapter::Chapter;
use crate::types::download::DownloadState;
use crate::types::generation_source::GenerationSource;
use crate::types::person::Person;
use crate::types::podcast::PodcastId;
use crate::types::soundbite::SoundBite;
use crate::types::transcript::{TranscriptKind, TranscriptState};
use crate::types::triage::TriageDecision;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EpisodeId(pub Uuid);

impl EpisodeId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: EpisodeId,
    pub podcast_id: PodcastId,
    pub guid: String,

    pub title: String,
    pub description: String,
    pub pub_date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,

    pub enclosure_url: Url,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosure_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<Chapter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persons: Option<Vec<Person>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_bites: Option<Vec<SoundBite>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_transcript_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_transcript_type: Option<TranscriptKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters_url: Option<Url>,

    pub position_secs: f64,
    pub played: bool,
    pub is_starred: bool,
    pub download_state: DownloadState,
    pub transcript_state: TranscriptState,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad_segments: Option<Vec<AdSegment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_source: Option<GenerationSource>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage_decision: Option<TriageDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage_rationale: Option<String>,
    pub triage_is_hero: bool,

    pub metadata_indexed: bool,
}

impl Episode {
    pub fn new(
        podcast_id: PodcastId,
        guid: impl Into<String>,
        title: impl Into<String>,
        enclosure_url: Url,
        pub_date: DateTime<Utc>,
    ) -> Self {
        Self {
            id: EpisodeId::generate(),
            podcast_id,
            guid: guid.into(),
            title: title.into(),
            description: String::new(),
            pub_date,
            duration_secs: None,
            enclosure_url,
            enclosure_mime_type: None,
            image_url: None,
            chapters: None,
            persons: None,
            sound_bites: None,
            publisher_transcript_url: None,
            publisher_transcript_type: None,
            chapters_url: None,
            position_secs: 0.0,
            played: false,
            is_starred: false,
            download_state: DownloadState::default(),
            transcript_state: TranscriptState::default(),
            ad_segments: None,
            generation_source: None,
            triage_decision: None,
            triage_rationale: None,
            triage_is_hero: false,
            metadata_indexed: false,
        }
    }

    pub fn id(&self) -> EpisodeId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Episode {
        Episode::new(
            PodcastId::generate(),
            "guid-1",
            "Pilot",
            Url::parse("https://example.com/audio.mp3").unwrap(),
            Utc::now(),
        )
    }

    #[test]
    fn episode_round_trip() {
        let value = fixture();
        let json = serde_json::to_string(&value).unwrap();
        let back: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn episode_with_chapters_round_trip() {
        let mut value = fixture();
        value.chapters = Some(vec![Chapter::new("Intro", 0.0)]);
        value.publisher_transcript_type = Some(TranscriptKind::Vtt);
        value.triage_decision = Some(TriageDecision::Inbox);
        value.triage_rationale = Some("Has the guest you follow".into());
        let json = serde_json::to_string(&value).unwrap();
        let back: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

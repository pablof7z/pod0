use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::episode::EpisodeId;
use crate::types::podcast::PodcastId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipSource {
    Touch,
    Auto,
    Headphone,
    Carplay,
    Watch,
    Siri,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub episode_id: EpisodeId,
    pub subscription_id: PodcastId,
    pub start_ms: i64,
    pub end_ms: i64,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
    pub transcript_text: String,
    pub source: ClipSource,
}

impl Clip {
    pub fn new(
        episode_id: EpisodeId,
        subscription_id: PodcastId,
        start_ms: i64,
        end_ms: i64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            episode_id,
            subscription_id,
            start_ms,
            end_ms,
            created_at: Utc::now(),
            caption: None,
            speaker_id: None,
            transcript_text: String::new(),
            source: ClipSource::Touch,
        }
    }

    pub fn duration_secs(&self) -> f64 {
        ((self.end_ms - self.start_ms).max(0)) as f64 / 1000.0
    }
}

/// Sentence-snapped clip boundary used by the composer / boundary resolver.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipBoundary {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_round_trip() {
        let value = Clip::new(EpisodeId::generate(), PodcastId::generate(), 1000, 4000);
        let json = serde_json::to_string(&value).unwrap();
        let back: Clip = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
        assert_eq!(value.duration_secs(), 3.0);
    }

    #[test]
    fn boundary_round_trip() {
        let value = ClipBoundary {
            start_ms: 0,
            end_ms: 5000,
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: ClipBoundary = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

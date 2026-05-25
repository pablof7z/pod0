use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::download::DownloadState;
use crate::types::episode::EpisodeId;
use crate::types::podcast::PodcastId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeSummary {
    pub id: EpisodeId,
    pub podcast_id: PodcastId,
    pub title: String,
    pub pub_date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    pub position_secs: f64,
    pub played: bool,
    pub download_state: DownloadState,
}

impl EpisodeSummary {
    pub fn new(
        id: EpisodeId,
        podcast_id: PodcastId,
        title: impl Into<String>,
        pub_date: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            podcast_id,
            title: title.into(),
            pub_date,
            duration_secs: None,
            position_secs: 0.0,
            played: false,
            download_state: DownloadState::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EpisodeProjection {
    pub episodes: Vec<EpisodeSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trip() {
        let mut value = EpisodeProjection::default();
        value.episodes.push(EpisodeSummary::new(
            EpisodeId::generate(),
            PodcastId::generate(),
            "Pilot",
            Utc::now(),
        ));
        let json = serde_json::to_string(&value).unwrap();
        let back: EpisodeProjection = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

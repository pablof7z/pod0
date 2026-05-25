use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::types::podcast::PodcastId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PodcastSummary {
    pub id: PodcastId,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,
    pub unplayed_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_episode_date: Option<DateTime<Utc>>,
}

impl PodcastSummary {
    pub fn new(id: PodcastId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            image_url: None,
            unplayed_count: 0,
            last_episode_date: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LibraryProjection {
    pub podcasts: Vec<PodcastSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn projection_round_trip() {
        let mut value = LibraryProjection::default();
        value
            .podcasts
            .push(PodcastSummary::new(PodcastId::new(Uuid::nil()), "Demo"));
        let json = serde_json::to_string(&value).unwrap();
        let back: LibraryProjection = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

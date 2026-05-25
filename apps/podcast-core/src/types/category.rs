use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::podcast::PodcastId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PodcastCategory {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_hex: Option<String>,
    pub subscription_ids: Vec<PodcastId>,
    pub generated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl PodcastCategory {
    pub fn new(name: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            slug: slug.into(),
            description: String::new(),
            color_hex: None,
            subscription_ids: Vec::new(),
            generated_at: Utc::now(),
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_round_trip() {
        let value = PodcastCategory::new("Tech Deep-Dives", "tech-deep-dives");
        let json = serde_json::to_string(&value).unwrap();
        let back: PodcastCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

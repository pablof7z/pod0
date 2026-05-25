use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: Uuid,
    pub start_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_url: Option<Url>,
    pub include_in_toc: bool,
    pub is_ai_generated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

impl Chapter {
    pub fn new(title: impl Into<String>, start_secs: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_secs,
            end_secs: None,
            title: title.into(),
            image_url: None,
            link_url: None,
            include_in_toc: true,
            is_ai_generated: false,
            summary: None,
            source_episode_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_round_trip() {
        let value = Chapter::new("Intro", 0.0);
        let json = serde_json::to_string(&value).unwrap();
        let back: Chapter = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

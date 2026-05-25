use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::subscription::AutoDownloadPolicy;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategorySettings {
    pub category_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_download_override: Option<AutoDownloadPolicy>,
    pub transcription_enabled: bool,
    pub rag_enabled: bool,
    pub wiki_generation_enabled: bool,
    pub briefings_enabled: bool,
    pub notifications_enabled: bool,
}

impl CategorySettings {
    pub fn default_for(category_id: Uuid) -> Self {
        Self {
            category_id,
            auto_download_override: None,
            transcription_enabled: true,
            rag_enabled: true,
            wiki_generation_enabled: true,
            briefings_enabled: true,
            notifications_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_settings_round_trip() {
        let value = CategorySettings::default_for(Uuid::nil());
        let json = serde_json::to_string(&value).unwrap();
        let back: CategorySettings = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}

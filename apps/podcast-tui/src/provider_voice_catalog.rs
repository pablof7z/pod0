use std::collections::BTreeMap;

use serde::Deserialize;

use crate::provider_model_catalog::ProviderCatalogModel;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct ProviderCatalogVoice {
    pub voice_id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderVoiceCatalogEnvelope {
    result: Option<ProviderVoiceCatalogResult>,
    error: Option<ProviderVoiceCatalogError>,
}

#[derive(Debug, Deserialize)]
struct ProviderVoiceCatalogResult {
    voices: Vec<ProviderCatalogVoice>,
}

#[derive(Debug, Deserialize)]
struct ProviderVoiceCatalogError {
    kind: Option<String>,
    message: Option<String>,
}

impl ProviderCatalogVoice {
    pub(crate) fn into_catalog_model(self) -> ProviderCatalogModel {
        let description = self.description();
        let search_text = self.search_text();
        ProviderCatalogModel {
            provider: "elevenlabs".to_owned(),
            id: self.voice_id.clone(),
            provider_model_id: self.voice_id.clone(),
            selection_model_id: self.voice_id,
            name: self.name,
            provider_name: "ElevenLabs".to_owned(),
            model_description: description,
            prompt_cost_per_million: None,
            completion_cost_per_million: None,
            input_modalities: vec!["text".to_owned()],
            output_modalities: vec!["audio".to_owned()],
            supports_tools: false,
            supports_reasoning: false,
            supports_response_format: false,
            supports_structured_outputs: false,
            search_text,
        }
    }

    fn description(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.category.trim().is_empty() {
            parts.push(self.category.clone());
        }
        for key in ["gender", "accent", "age", "use_case", "description"] {
            if let Some(value) = self
                .labels
                .get(key)
                .filter(|value| !value.trim().is_empty())
            {
                parts.push(value.replace('_', " "));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }

    fn search_text(&self) -> String {
        let mut parts = vec![
            self.voice_id.clone(),
            self.name.clone(),
            self.category.clone(),
        ];
        parts.extend(self.labels.values().cloned());
        parts.join(" ").to_lowercase()
    }
}

pub(crate) fn decode_elevenlabs_voice_catalog(
    json: &str,
) -> Result<Vec<ProviderCatalogVoice>, String> {
    let envelope: ProviderVoiceCatalogEnvelope =
        serde_json::from_str(json).map_err(|e| format!("voice catalog JSON: {e}"))?;
    if let Some(error) = envelope.error {
        return Err(error
            .message
            .or(error.kind)
            .unwrap_or_else(|| "voice catalog error".to_owned()));
    }
    envelope
        .result
        .map(|result| result.voices)
        .ok_or_else(|| "voice catalog response missing result".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_voice_catalog_envelope() {
        let json = r#"{"result":{"provider":"elevenlabs","voices":[{
            "voice_id":"v1",
            "name":"Narrator",
            "category":"premade",
            "labels":{"gender":"female","accent":"american"},
            "preview_url":"https://example.test/preview.mp3"
        }]}}"#;
        let voices = decode_elevenlabs_voice_catalog(json).unwrap();
        let model = voices[0].clone().into_catalog_model();
        assert_eq!(model.selection_id(), "v1");
        assert_eq!(model.display_name(), "Narrator");
        assert!(model.matches_query("american"));
        assert!(model.output_modalities.iter().any(|item| item == "audio"));
        assert_eq!(
            crate::provider_model_catalog::visible_provider_models(
                &[model],
                Some(crate::provider_settings_catalog::ProviderSettingItem::ElevenLabsVoice),
                ""
            )
            .len(),
            1
        );
    }
}

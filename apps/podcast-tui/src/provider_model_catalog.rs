use serde::Deserialize;

use crate::provider_settings_catalog::ProviderSettingItem;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct ProviderCatalogModel {
    pub provider: String,
    pub id: String,
    #[serde(default)]
    pub provider_model_id: String,
    #[serde(default)]
    pub selection_model_id: String,
    pub name: String,
    pub provider_name: String,
    pub model_description: Option<String>,
    pub prompt_cost_per_million: Option<f64>,
    pub completion_cost_per_million: Option<f64>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub supports_response_format: bool,
    pub supports_structured_outputs: bool,
    pub search_text: String,
}

#[derive(Debug, Deserialize)]
struct ProviderCatalogEnvelope {
    result: Option<ProviderCatalogResult>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderCatalogResult {
    models: Vec<ProviderCatalogModel>,
}

impl ProviderCatalogModel {
    pub(crate) fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.id
        } else {
            &self.name
        }
    }

    pub(crate) fn selection_id(&self) -> &str {
        if self.selection_model_id.trim().is_empty() {
            &self.id
        } else {
            &self.selection_model_id
        }
    }

    pub(crate) fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.search_text.contains(&query)
            || self.id.to_lowercase().contains(&query)
            || self.provider_model_id.to_lowercase().contains(&query)
            || self.selection_id().to_lowercase().contains(&query)
            || self.name.to_lowercase().contains(&query)
            || self.provider_name.to_lowercase().contains(&query)
    }

    pub(crate) fn is_image_output(&self) -> bool {
        self.output_modalities.iter().any(|item| item == "image")
    }

    pub(crate) fn is_audio_output(&self) -> bool {
        self.output_modalities.iter().any(|item| item == "audio")
    }

    pub(crate) fn is_text_output(&self) -> bool {
        self.output_modalities.is_empty()
            || self.output_modalities.iter().any(|item| item == "text")
    }

    pub(crate) fn compact_flags(&self) -> String {
        let mut flags = Vec::new();
        if self.supports_tools {
            flags.push("tools");
        }
        if self.supports_reasoning {
            flags.push("reasoning");
        }
        if self.supports_response_format || self.supports_structured_outputs {
            flags.push("json");
        }
        if self.is_image_output() {
            flags.push("image");
        }
        if self.is_audio_output() {
            flags.push("voice");
        }
        if flags.is_empty() {
            "basic".to_owned()
        } else {
            flags.join(", ")
        }
    }

    pub(crate) fn compact_price(&self) -> String {
        match (
            self.prompt_cost_per_million,
            self.completion_cost_per_million,
        ) {
            (Some(0.0), Some(0.0)) => "free".to_owned(),
            (Some(input), Some(output)) => format!("${input:.2}/${output:.2} per 1M"),
            _ if self.is_audio_output() => "voice".to_owned(),
            _ => "variable".to_owned(),
        }
    }
}

pub(crate) fn decode_provider_catalog(json: &str) -> Result<Vec<ProviderCatalogModel>, String> {
    let envelope: ProviderCatalogEnvelope =
        serde_json::from_str(json).map_err(|e| format!("provider catalog JSON: {e}"))?;
    if let Some(error) = envelope.error {
        return Err(error);
    }
    envelope
        .result
        .map(|result| result.models)
        .ok_or_else(|| "provider catalog response missing result".to_owned())
}

pub(crate) fn visible_provider_models<'a>(
    models: &'a [ProviderCatalogModel],
    target: Option<ProviderSettingItem>,
    query: &str,
) -> Vec<(usize, &'a ProviderCatalogModel)> {
    models
        .iter()
        .enumerate()
        .filter(|(_, model)| matches_target(model, target) && model.matches_query(query))
        .collect()
}

fn matches_target(model: &ProviderCatalogModel, target: Option<ProviderSettingItem>) -> bool {
    match target {
        Some(item) if item.is_image_model_setting() => model.is_image_output(),
        Some(ProviderSettingItem::ElevenLabsVoice) => {
            model.provider == "elevenlabs" && model.is_audio_output()
        }
        Some(_) => model.is_text_output(),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_catalog_envelope() {
        let json = r#"{"result":{"models":[{
            "provider":"openrouter",
            "id":"openai/gpt-4o",
            "provider_model_id":"openai/gpt-4o",
            "selection_model_id":"openrouter:openai/gpt-4o",
            "name":"GPT-4o",
            "provider_name":"OpenAI",
            "model_description":"Fast",
            "prompt_cost_per_million":2.5,
            "completion_cost_per_million":10.0,
            "input_modalities":["text"],
            "output_modalities":["text"],
            "supports_tools":true,
            "supports_reasoning":false,
            "supports_response_format":true,
            "supports_structured_outputs":false,
            "search_text":"openai gpt-4o"
        }]}}"#;
        let models = decode_provider_catalog(json).unwrap();
        assert_eq!(models[0].display_name(), "GPT-4o");
        assert_eq!(models[0].selection_id(), "openrouter:openai/gpt-4o");
        assert!(models[0].matches_query("openai"));
        assert!(models[0].matches_query("openrouter"));
    }
}

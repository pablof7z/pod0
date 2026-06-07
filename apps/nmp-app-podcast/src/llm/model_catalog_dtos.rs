//! Provider catalog wire DTOs and normalization.

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

use super::model_catalog::ProviderModelOption;

#[derive(Debug, Deserialize)]
pub(super) struct OrModelsResponse {
    pub data: Vec<OrModel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OrModel {
    id: String,
    name: String,
    created: Option<i64>,
    description: Option<String>,
    #[serde(rename = "context_length")]
    context_length: Option<i64>,
    architecture: Option<OrArchitecture>,
    pricing: Option<OrPricing>,
    #[serde(rename = "top_provider")]
    top_provider: Option<OrTopProvider>,
    #[serde(rename = "supported_parameters")]
    supported_parameters: Option<Vec<String>>,
    #[serde(rename = "knowledge_cutoff")]
    knowledge_cutoff: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrArchitecture {
    #[serde(rename = "input_modalities")]
    input_modalities: Option<Vec<String>>,
    #[serde(rename = "output_modalities")]
    output_modalities: Option<Vec<String>>,
    tokenizer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrPricing {
    prompt: Option<String>,
    completion: Option<String>,
    request: Option<String>,
    image: Option<String>,
    #[serde(rename = "web_search")]
    web_search: Option<String>,
    #[serde(rename = "input_cache_read")]
    input_cache_read: Option<String>,
    #[serde(rename = "input_cache_write")]
    input_cache_write: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrTopProvider {
    #[serde(rename = "context_length")]
    context_length: Option<i64>,
    #[serde(rename = "max_completion_tokens")]
    max_completion_tokens: Option<i64>,
    #[serde(rename = "is_moderated")]
    is_moderated: Option<bool>,
}

impl OrModel {
    pub(super) fn into_option(self, metadata: Option<&ModelsDevCatalog>) -> ProviderModelOption {
        let provider_id = provider_id_from_model_id(&self.id);
        let dev_model = metadata.and_then(|catalog| catalog.openrouter_model(&self.id));
        let provider = metadata.and_then(|catalog| catalog.provider(&provider_id));
        let supported = self.supported_parameters.unwrap_or_default();
        let input_modalities = self
            .architecture
            .as_ref()
            .and_then(|arch| arch.input_modalities.clone())
            .or_else(|| dev_model.and_then(|model| model.modalities.as_ref()?.input.clone()))
            .unwrap_or_default();
        let output_modalities = self
            .architecture
            .as_ref()
            .and_then(|arch| arch.output_modalities.clone())
            .or_else(|| dev_model.and_then(|model| model.modalities.as_ref()?.output.clone()))
            .unwrap_or_default();
        let pricing = self.pricing.as_ref();
        let top_provider = self.top_provider.as_ref();
        let supports_structured_outputs = supported.iter().any(|item| item == "structured_outputs")
            || dev_model
                .and_then(|model| model.structured_output)
                .unwrap_or(false);
        let supports_response_format =
            supported.iter().any(|item| item == "response_format") || supports_structured_outputs;
        let provider_name = provider
            .map(|provider| provider.name.clone())
            .unwrap_or_else(|| provider_name_from_model_name(&self.name, &provider_id));
        let mut option = ProviderModelOption {
            provider: "openrouter",
            id: self.id.clone(),
            name: self.name,
            provider_id,
            provider_name,
            provider_icon_url: provider.and_then(|provider| provider.icon.clone()),
            model_description: self.description,
            prompt_cost_per_million: pricing
                .and_then(|pricing| cost_per_million(pricing.prompt.as_deref()))
                .or_else(|| dev_model.and_then(|model| model.cost.as_ref()?.input)),
            completion_cost_per_million: pricing
                .and_then(|pricing| cost_per_million(pricing.completion.as_deref()))
                .or_else(|| dev_model.and_then(|model| model.cost.as_ref()?.output)),
            cache_read_cost_per_million: pricing
                .and_then(|pricing| cost_per_million(pricing.input_cache_read.as_deref()))
                .or_else(|| dev_model.and_then(|model| model.cost.as_ref()?.cache_read)),
            cache_write_cost_per_million: pricing
                .and_then(|pricing| cost_per_million(pricing.input_cache_write.as_deref()))
                .or_else(|| dev_model.and_then(|model| model.cost.as_ref()?.cache_write)),
            request_cost: pricing.and_then(|pricing| parse_nonnegative(pricing.request.as_deref())),
            image_cost: pricing.and_then(|pricing| parse_nonnegative(pricing.image.as_deref())),
            web_search_cost: pricing
                .and_then(|pricing| parse_nonnegative(pricing.web_search.as_deref())),
            context_length: self
                .context_length
                .or_else(|| top_provider.and_then(|provider| provider.context_length))
                .or_else(|| dev_model.and_then(|model| model.limit.as_ref()?.context)),
            output_limit: top_provider
                .and_then(|provider| provider.max_completion_tokens)
                .or_else(|| dev_model.and_then(|model| model.limit.as_ref()?.output)),
            input_modalities,
            output_modalities,
            tokenizer: self.architecture.and_then(|arch| arch.tokenizer),
            supports_tools: supported.iter().any(|item| item == "tools")
                || dev_model.and_then(|model| model.tool_call).unwrap_or(false),
            supports_reasoning: supported.iter().any(|item| item.contains("reasoning"))
                || dev_model.and_then(|model| model.reasoning).unwrap_or(false),
            supports_response_format,
            supports_structured_outputs,
            open_weights: dev_model
                .and_then(|model| model.open_weights)
                .unwrap_or(false),
            is_moderated: top_provider.and_then(|provider| provider.is_moderated),
            created_at_epoch_secs: self.created,
            knowledge_cutoff: self
                .knowledge_cutoff
                .or_else(|| dev_model.and_then(|model| model.knowledge.clone())),
            release_date: dev_model.and_then(|model| model.release_date.clone()),
            last_updated: dev_model.and_then(|model| model.last_updated.clone()),
            search_text: String::new(),
        };
        option.search_text = make_search_text(&option);
        option
    }
}

#[derive(Debug)]
pub(super) struct ModelsDevCatalog {
    pub providers: HashMap<String, ModelsDevProvider>,
}

impl ModelsDevCatalog {
    fn provider(&self, id: &str) -> Option<&ModelsDevProvider> {
        self.providers.get(id)
    }

    fn openrouter_model(&self, id: &str) -> Option<&ModelsDevModel> {
        self.providers.get("openrouter")?.models.get(id)
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct ModelsDevProvider {
    name: String,
    icon: Option<String>,
    models: HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    reasoning: Option<bool>,
    #[serde(rename = "tool_call")]
    tool_call: Option<bool>,
    #[serde(rename = "structured_output")]
    structured_output: Option<bool>,
    knowledge: Option<String>,
    #[serde(rename = "release_date")]
    release_date: Option<String>,
    #[serde(rename = "last_updated")]
    last_updated: Option<String>,
    modalities: Option<ModelsDevModalities>,
    #[serde(rename = "open_weights")]
    open_weights: Option<bool>,
    cost: Option<ModelsDevCost>,
    limit: Option<ModelsDevLimit>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModalities {
    input: Option<Vec<String>>,
    output: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevCost {
    input: Option<f64>,
    output: Option<f64>,
    #[serde(rename = "cache_read")]
    cache_read: Option<f64>,
    #[serde(rename = "cache_write")]
    cache_write: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    context: Option<i64>,
    output: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OllamaTagsResponse {
    pub models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OllamaTagModel {
    name: String,
    model: Option<String>,
    #[serde(rename = "modified_at")]
    modified_at: Option<String>,
    details: Option<OllamaDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaDetails {
    family: Option<String>,
    families: Option<Vec<String>>,
    #[serde(rename = "parameter_size")]
    parameter_size: Option<String>,
    #[serde(rename = "quantization_level")]
    quantization_level: Option<String>,
}

impl OllamaTagModel {
    pub(super) fn into_option(self) -> ProviderModelOption {
        let raw_id = self.model.clone().unwrap_or_else(|| self.name.clone());
        let family = self
            .details
            .as_ref()
            .and_then(|details| details.family.clone())
            .or_else(|| self.details.as_ref()?.families.as_ref()?.first().cloned())
            .unwrap_or_else(|| "ollama".to_owned());
        let detail_parts = self
            .details
            .as_ref()
            .map(|details| {
                [
                    details.parameter_size.as_deref(),
                    details.quantization_level.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ")
            })
            .unwrap_or_default();
        let parsed_modified_at = self.modified_at.as_deref().and_then(parse_ollama_date);
        let model_description = if detail_parts.is_empty() {
            "Cloud model available through Ollama's hosted API.".to_owned()
        } else {
            format!("Cloud model available through Ollama's hosted API. {detail_parts}.")
        };
        let mut option = ProviderModelOption {
            provider: "ollama",
            id: format!("ollama:{raw_id}"),
            name: self.name,
            provider_id: "ollama-cloud".to_owned(),
            provider_name: "Ollama Cloud".to_owned(),
            provider_icon_url: None,
            model_description: Some(model_description),
            prompt_cost_per_million: None,
            completion_cost_per_million: None,
            cache_read_cost_per_million: None,
            cache_write_cost_per_million: None,
            request_cost: None,
            image_cost: None,
            web_search_cost: None,
            context_length: None,
            output_limit: None,
            input_modalities: vec!["text".to_owned()],
            output_modalities: vec!["text".to_owned()],
            tokenizer: Some(family),
            supports_tools: true,
            supports_reasoning: raw_id.to_lowercase().contains("gpt-oss")
                || raw_id.to_lowercase().contains("qwen"),
            supports_response_format: true,
            supports_structured_outputs: true,
            open_weights: true,
            is_moderated: None,
            created_at_epoch_secs: parsed_modified_at.map(|date| date.timestamp()),
            knowledge_cutoff: None,
            release_date: None,
            last_updated: parsed_modified_at.map(|date| date.format("%Y-%m-%d").to_string()),
            search_text: String::new(),
        };
        option.search_text = make_search_text(&option);
        option
    }
}

fn provider_id_from_model_id(model_id: &str) -> String {
    model_id
        .split_once('/')
        .map(|(provider, _)| provider)
        .unwrap_or("openrouter")
        .to_owned()
}

fn provider_name_from_model_name(model_name: &str, provider_id: &str) -> String {
    if let Some((provider, _)) = model_name.split_once(':') {
        return provider.to_owned();
    }
    provider_id
        .replace('-', " ")
        .split_whitespace()
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn parse_nonnegative(value: Option<&str>) -> Option<f64> {
    let value = value?.parse::<f64>().ok()?;
    (value >= 0.0).then_some(value)
}

fn cost_per_million(value: Option<&str>) -> Option<f64> {
    parse_nonnegative(value).map(|value| value * 1_000_000.0)
}

fn parse_ollama_date(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}

fn make_search_text(option: &ProviderModelOption) -> String {
    [
        option.id.as_str(),
        option.name.as_str(),
        option.provider_name.as_str(),
        option.provider_id.as_str(),
        option.model_description.as_deref().unwrap_or_default(),
        option.tokenizer.as_deref().unwrap_or_default(),
        &option.input_modalities.join(" "),
        &option.output_modalities.join(" "),
    ]
    .join(" ")
    .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_option_uses_models_dev_metadata() {
        let mut providers = HashMap::new();
        providers.insert(
            "openrouter".to_owned(),
            ModelsDevProvider {
                name: "OpenRouter".to_owned(),
                icon: None,
                models: [(
                    "openai/gpt-4o".to_owned(),
                    ModelsDevModel {
                        reasoning: Some(true),
                        tool_call: Some(true),
                        structured_output: Some(true),
                        knowledge: Some("2024-04".to_owned()),
                        release_date: Some("2024-05-13".to_owned()),
                        last_updated: Some("2025-01-01".to_owned()),
                        modalities: Some(ModelsDevModalities {
                            input: Some(vec!["text".to_owned(), "image".to_owned()]),
                            output: Some(vec!["text".to_owned()]),
                        }),
                        open_weights: Some(false),
                        cost: Some(ModelsDevCost {
                            input: Some(2.5),
                            output: Some(10.0),
                            cache_read: None,
                            cache_write: None,
                        }),
                        limit: Some(ModelsDevLimit {
                            context: Some(128_000),
                            output: Some(4096),
                        }),
                    },
                )]
                .into_iter()
                .collect(),
            },
        );
        providers.insert(
            "openai".to_owned(),
            ModelsDevProvider {
                name: "OpenAI".to_owned(),
                icon: Some("https://example.com/openai.svg".to_owned()),
                models: HashMap::new(),
            },
        );
        let option = OrModel {
            id: "openai/gpt-4o".to_owned(),
            name: "GPT-4o".to_owned(),
            created: Some(1_700_000_000),
            description: Some("Fast flagship".to_owned()),
            context_length: None,
            architecture: None,
            pricing: None,
            top_provider: None,
            supported_parameters: Some(vec!["response_format".to_owned()]),
            knowledge_cutoff: None,
        }
        .into_option(Some(&ModelsDevCatalog { providers }));

        assert_eq!(option.provider, "openrouter");
        assert_eq!(option.id, "openai/gpt-4o");
        assert_eq!(option.provider_name, "OpenAI");
        assert_eq!(
            option.provider_icon_url.as_deref(),
            Some("https://example.com/openai.svg")
        );
        assert_eq!(option.prompt_cost_per_million, Some(2.5));
        assert!(option.supports_tools);
        assert!(option.supports_response_format);
        assert!(option.search_text.contains("gpt-4o"));
    }

    #[test]
    fn ollama_option_uses_provider_prefix_and_reasoning_flags() {
        let option = OllamaTagModel {
            name: "qwen3:cloud".to_owned(),
            model: None,
            modified_at: Some("2025-05-01T12:00:00Z".to_owned()),
            details: Some(OllamaDetails {
                family: None,
                families: Some(vec!["qwen".to_owned()]),
                parameter_size: Some("32B".to_owned()),
                quantization_level: Some("Q4_K_M".to_owned()),
            }),
        }
        .into_option();

        assert_eq!(option.provider, "ollama");
        assert_eq!(option.id, "ollama:qwen3:cloud");
        assert_eq!(option.tokenizer.as_deref(), Some("qwen"));
        assert!(option.supports_reasoning);
        assert_eq!(option.last_updated.as_deref(), Some("2025-05-01"));
    }
}

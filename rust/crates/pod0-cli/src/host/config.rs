use crate::protocol::{AgentProvider, CapabilityStatus, CliError};

#[derive(Clone)]
pub struct HostConfig {
    pub(super) openai_base_url: Option<String>,
    pub(super) openai_api_key: Option<String>,
    pub(super) ollama_base_url: Option<String>,
    default_provider: Option<AgentProvider>,
    default_model: Option<String>,
}

impl HostConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            openai_base_url: first_env(&["POD0_OPENAI_BASE_URL", "OPENAI_BASE_URL"]),
            openai_api_key: first_env(&["POD0_OPENAI_API_KEY", "OPENAI_API_KEY"]),
            ollama_base_url: first_env(&["POD0_OLLAMA_BASE_URL", "OLLAMA_HOST"])
                .map(normalize_ollama_url),
            default_provider: std::env::var("POD0_AGENT_PROVIDER")
                .ok()
                .and_then(|value| parse_provider(&value)),
            default_model: std::env::var("POD0_AGENT_MODEL").ok(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            openai_base_url: None,
            openai_api_key: None,
            ollama_base_url: None,
            default_provider: None,
            default_model: None,
        }
    }

    #[must_use]
    pub fn openai_compatible(base_url: String, api_key: Option<String>) -> Self {
        Self {
            openai_base_url: Some(base_url),
            openai_api_key: api_key,
            ollama_base_url: None,
            default_provider: Some(AgentProvider::OpenAiCompatible),
            default_model: None,
        }
    }

    #[must_use]
    pub fn ollama(base_url: String) -> Self {
        Self {
            openai_base_url: None,
            openai_api_key: None,
            ollama_base_url: Some(normalize_ollama_url(base_url)),
            default_provider: Some(AgentProvider::Ollama),
            default_model: None,
        }
    }

    pub(crate) fn capabilities(&self) -> CapabilityStatus {
        CapabilityStatus {
            feed_http: true,
            library_http: true,
            openai_compatible: self.openai_base_url.is_some(),
            ollama: self.ollama_base_url.is_some(),
            agent_tools: false,
            agent_capability_execution: false,
            audio_playback: true,
            clip_media: false,
        }
    }

    pub(crate) fn resolve_model(
        &self,
        provider: Option<AgentProvider>,
        model: Option<String>,
    ) -> Result<ModelTarget, CliError> {
        let provider = provider
            .or(self.default_provider)
            .or(match (&self.openai_base_url, &self.ollama_base_url) {
                (Some(_), None) => Some(AgentProvider::OpenAiCompatible),
                (None, Some(_)) => Some(AgentProvider::Ollama),
                _ => None,
            })
            .ok_or_else(|| {
                CliError::new(
                    "agent_provider_missing",
                    "configure an OpenAI-compatible or Ollama endpoint",
                    false,
                )
            })?;
        let model = model
            .or_else(|| self.default_model.clone())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CliError::new("agent_model_missing", "configure an agent model", false)
            })?;
        let endpoint_configured = match provider {
            AgentProvider::OpenAiCompatible => self.openai_base_url.is_some(),
            AgentProvider::Ollama => self.ollama_base_url.is_some(),
        };
        if !endpoint_configured {
            return Err(CliError::new(
                "agent_provider_missing",
                "the selected agent provider endpoint is not configured",
                false,
            ));
        }
        let prefix = match provider {
            AgentProvider::OpenAiCompatible => "openai",
            AgentProvider::Ollama => "ollama",
        };
        let model = normalize_model(prefix, &model)?;
        Ok(ModelTarget {
            model_reference: format!("{prefix}:{model}"),
        })
    }
}

impl Default for HostConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

#[derive(Debug)]
pub(crate) struct ModelTarget {
    pub(crate) model_reference: String,
}

fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty())
}

fn parse_provider(value: &str) -> Option<AgentProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "openai_compatible" => Some(AgentProvider::OpenAiCompatible),
        "ollama" => Some(AgentProvider::Ollama),
        _ => None,
    }
}

fn normalize_ollama_url(value: String) -> String {
    if value.contains("://") {
        value
    } else {
        format!("http://{value}")
    }
}

fn normalize_model<'a>(provider: &str, model: &'a str) -> Result<&'a str, CliError> {
    for known in ["openai", "ollama"] {
        if let Some(value) = model.strip_prefix(&format!("{known}:")) {
            if known != provider || value.is_empty() {
                return Err(CliError::new(
                    "agent_model_invalid",
                    "agent model provider prefix does not match the selected provider",
                    false,
                ));
            }
            return Ok(value);
        }
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_provider_is_reported_without_success() {
        let error = HostConfig::empty()
            .resolve_model(None, Some("model".to_owned()))
            .expect_err("missing provider must fail");
        assert_eq!(error.code, "agent_provider_missing");
    }

    #[test]
    fn capability_output_never_contains_credentials_or_endpoints() {
        let config = HostConfig {
            openai_base_url: Some("https://secret-host.example/v1".to_owned()),
            openai_api_key: Some("top-secret-key".to_owned()),
            ollama_base_url: None,
            default_provider: None,
            default_model: None,
        };
        let output = serde_json::to_string(&config.capabilities()).unwrap();
        assert!(!output.contains("top-secret-key"));
        assert!(!output.contains("secret-host.example"));
        assert!(!config.capabilities().agent_tools);
        assert!(!config.capabilities().agent_capability_execution);
    }
}

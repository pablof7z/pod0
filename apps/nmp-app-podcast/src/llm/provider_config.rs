//! Shared provider runtime configuration.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::store::PodcastStore;
use url::Url;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com";
pub const DEFAULT_OLLAMA_CHAT_URL: &str = "https://ollama.com/api/chat";
pub const ELEVENLABS_BASE_URL: &str = "https://api.elevenlabs.io";
pub const ASSEMBLYAI_BASE_URL: &str = "https://api.assemblyai.com";

#[derive(Debug)]
pub enum ProviderConfigError {
    StoreUnavailable,
}

#[derive(Clone)]
pub struct ProviderSettings {
    pub openrouter_key: Option<String>,
    pub ollama_key: Option<String>,
    pub eleven_labs_key: Option<String>,
    pub assembly_ai_key: Option<String>,
    pub perplexity_key: Option<String>,
    pub ollama_base_url: String,
    pub openrouter_whisper_model: String,
    pub eleven_labs_stt_model: String,
    pub eleven_labs_tts_model: String,
    pub assembly_ai_stt_model: String,
}

impl ProviderSettings {
    pub fn from_store(store: &Arc<Mutex<PodcastStore>>) -> Result<Self, ProviderConfigError> {
        let store = store
            .lock()
            .map_err(|_| ProviderConfigError::StoreUnavailable)?;
        Ok(Self {
            openrouter_key: store.open_router_api_key().map(str::to_owned),
            ollama_key: store.ollama_api_key().map(str::to_owned),
            eleven_labs_key: store.eleven_labs_api_key().map(str::to_owned),
            assembly_ai_key: store.assembly_ai_api_key().map(str::to_owned),
            perplexity_key: store.perplexity_api_key().map(str::to_owned),
            ollama_base_url: ollama_base_url_from_chat_url(store.ollama_chat_url()),
            openrouter_whisper_model: store.open_router_whisper_model().to_owned(),
            eleven_labs_stt_model: store.eleven_labs_stt_model().to_owned(),
            eleven_labs_tts_model: store.eleven_labs_tts_model().to_owned(),
            assembly_ai_stt_model: store.assembly_ai_stt_model().to_owned(),
        })
    }
}

pub fn strip_provider_prefix<'a>(model: &'a str, provider: &str) -> &'a str {
    model
        .strip_prefix(provider)
        .and_then(|rest| rest.strip_prefix(':'))
        .unwrap_or(model)
}

pub fn ollama_base_url_from_chat_url(chat_url: &str) -> String {
    normalize_ollama_chat_url(chat_url)
        .trim_end_matches("/api/chat")
        .to_owned()
}

pub fn normalize_ollama_chat_url(chat_url: &str) -> String {
    let trimmed = chat_url.trim();
    if trimmed.is_empty() {
        return DEFAULT_OLLAMA_CHAT_URL.to_owned();
    }

    let mut url = match Url::parse(trimmed) {
        Ok(url) => url,
        Err(_) => return DEFAULT_OLLAMA_CHAT_URL.to_owned(),
    };

    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return DEFAULT_OLLAMA_CHAT_URL.to_owned();
    }

    if url
        .host_str()
        .map(|host| host.eq_ignore_ascii_case("localhost"))
        .unwrap_or(false)
    {
        let _ = url.set_host(Some("127.0.0.1"));
    }

    url.set_query(None);
    url.set_fragment(None);

    let path = url.path().trim_end_matches('/');
    let normalized_path = if path.is_empty() || path == "/" {
        "/api/chat".to_owned()
    } else if path.ends_with("/api/chat") {
        path.to_owned()
    } else {
        format!("{path}/api/chat")
    };
    url.set_path(&normalized_path);
    url.to_string()
}

pub fn ollama_chat_url(base_url: &str) -> String {
    format!("{}/api/chat", base_url.trim_end_matches('/'))
}

pub fn ollama_embed_url(base_url: &str) -> String {
    format!("{}/api/embed", base_url.trim_end_matches('/'))
}

pub fn ollama_tags_url(base_url: &str) -> String {
    format!("{}/api/tags", base_url.trim_end_matches('/'))
}

pub fn is_ollama_cloud_base_url(base_url: &str) -> bool {
    base_url.trim_end_matches('/') == OLLAMA_CLOUD_BASE_URL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_provider_prefix() {
        assert_eq!(
            strip_provider_prefix("openrouter:openai/gpt-4o", "openrouter"),
            "openai/gpt-4o"
        );
        assert_eq!(
            strip_provider_prefix("ollama:gpt-oss:120b-cloud", "ollama"),
            "gpt-oss:120b-cloud"
        );
        assert_eq!(
            strip_provider_prefix("openai/gpt-4o", "openrouter"),
            "openai/gpt-4o"
        );
    }

    #[test]
    fn derives_ollama_urls_from_chat_setting() {
        let base = ollama_base_url_from_chat_url("http://localhost:11434/api/chat");
        assert_eq!(base, "http://127.0.0.1:11434");
        assert_eq!(ollama_chat_url(&base), "http://127.0.0.1:11434/api/chat");
        assert_eq!(ollama_embed_url(&base), "http://127.0.0.1:11434/api/embed");
        assert_eq!(ollama_tags_url(&base), "http://127.0.0.1:11434/api/tags");
    }

    #[test]
    fn normalizes_ollama_chat_url_variants() {
        assert_eq!(
            normalize_ollama_chat_url(" http://localhost:11434 "),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            normalize_ollama_chat_url("https://ollama.com/api/chat/"),
            DEFAULT_OLLAMA_CHAT_URL
        );
        assert_eq!(
            normalize_ollama_chat_url("not a url"),
            DEFAULT_OLLAMA_CHAT_URL
        );
    }
}

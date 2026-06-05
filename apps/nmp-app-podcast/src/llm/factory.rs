//! Factory for selecting and constructing the appropriate LLM backend.

use std::sync::{Arc, Mutex};

use super::backend::LlmBackend;
use super::local_model_backend::LocalModelBackend;
use super::ollama_backend::OllamaBackend;
use super::openrouter_backend::OpenRouterBackend;
use crate::store::PodcastStore;

/// Default Ollama base URL (Ollama Cloud). Used when the store has no URL configured.
pub const DEFAULT_OLLAMA_BASE_URL: &str = "https://ollama.com";

/// Default OpenRouter base URL.
pub const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Derive the base URL from the stored full chat URL.
///
/// The store holds the complete endpoint (e.g. `https://ollama.com/api/chat`)
/// while rig-core's `base_url` wants just the host root. Strip `/api/chat`
/// if present; fall back to the cloud default for empty values.
fn base_url_from_chat_url(chat_url: &str) -> String {
    let trimmed = chat_url.trim_end_matches("/api/chat");
    if trimmed.is_empty() {
        DEFAULT_OLLAMA_BASE_URL.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Accessor helpers for OpenRouter and Ollama API keys.
/// These delegate to the real accessors on PodcastStore.
fn open_router_api_key(store: &PodcastStore) -> Option<String> {
    store.open_router_api_key().map(|s| s.to_owned())
}

fn ollama_api_key(store: &PodcastStore) -> Option<String> {
    store.ollama_api_key().map(|s| s.to_owned())
}

/// Resolve the model string a role should actually run, honoring an on-device
/// selection while keeping cloud behavior byte-identical.
///
/// Per-role selection (PR #253) stores each role's chosen model, but the call
/// sites historically passed a hardcoded cloud constant — so the selection
/// never reached `backend_for` for any provider. This gate completes the
/// *local* half only: if the role's configured model is a `local:<id>`
/// selection, it is returned (so `backend_for` routes the role on-device and
/// the request carries the local id); otherwise the caller's existing cloud
/// default is returned unchanged. Wiring the cloud-to-cloud per-role path (and
/// aligning the pro/flash defaults) is a separate follow-up that finishes #253.
pub fn role_model_or_default(configured: &str, cloud_default: &str) -> String {
    if configured.starts_with("local:") {
        configured.to_owned()
    } else {
        cloud_default.to_owned()
    }
}

/// Provider-blind backend selection + key injection.
///
/// Reads the credential source / model prefix from the store and the in-memory
/// pushed key, then returns the right boxed backend.
///
/// Selection rule (per-role — keyed on the caller's own model string):
/// - If the model string carries a `local:` prefix, use LocalModelBackend for
///   that role only. "Local" is just another provider the user picks per role,
///   not a global override — so a role on `local:gemma4-e2b` runs on-device
///   while a sibling role on an OpenRouter model still hits the cloud.
/// - Else if the model string carries an `openrouter:` prefix, use OpenRouter.
/// - Else if `store.open_router_credential_source()` indicates a connected
///   OpenRouter source for non-Ollama models, use OpenRouter.
/// - Else use Ollama (the default).
///
/// `store.local_model_id()` is no longer a routing input — it now only signals
/// which single on-device engine the host should keep loaded (the host derives
/// it from the set of role selections; only one local engine loads at a time).
pub fn backend_for(
    store: &Arc<Mutex<PodcastStore>>,
    model: &str,
) -> Box<dyn LlmBackend> {
    // Per-role local routing: a `local:<id>` model string targets the
    // on-device backend for this caller only.
    if let Some(id) = model.strip_prefix("local:") {
        return Box::new(LocalModelBackend { model_id: id.to_owned() });
    }

    let use_openrouter = if model.starts_with("openrouter:") {
        true
    } else {
        store
            .lock()
            .map(|s| !s.open_router_credential_source().is_empty())
            .unwrap_or(false)
    };

    if use_openrouter {
        let api_key = store
            .lock()
            .ok()
            .and_then(|s| open_router_api_key(&s))
            .unwrap_or_default();

        Box::new(OpenRouterBackend {
            base_url: DEFAULT_OPENROUTER_BASE_URL.to_owned(),
            api_key,
        })
    } else {
        let base_url = store
            .lock()
            .map(|s| base_url_from_chat_url(s.ollama_chat_url()))
            .unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_owned());

        let api_key = store
            .lock()
            .ok()
            .and_then(|s| ollama_api_key(&s));

        Box::new(OllamaBackend { base_url, api_key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_model_gate_passes_local_selection() {
        // A local selection is returned so the role routes on-device.
        assert_eq!(
            role_model_or_default("local:gemma4-e2b", "deepseek-v4-pro:cloud"),
            "local:gemma4-e2b"
        );
    }

    #[test]
    fn test_role_model_gate_keeps_cloud_default_unchanged() {
        // Non-local configs keep the caller's hardcoded cloud default — zero
        // cloud regression (cloud per-role wiring is a separate follow-up).
        assert_eq!(
            role_model_or_default("openrouter:openai/gpt-4o", "deepseek-v4-pro:cloud"),
            "deepseek-v4-pro:cloud"
        );
        assert_eq!(
            role_model_or_default("", "deepseek-v4-flash:cloud"),
            "deepseek-v4-flash:cloud"
        );
    }

    #[test]
    fn test_base_url_from_chat_url_with_api_chat() {
        let url = "https://ollama.com/api/chat";
        assert_eq!(
            base_url_from_chat_url(url),
            "https://ollama.com"
        );
    }

    #[test]
    fn test_base_url_from_chat_url_without_suffix() {
        let url = "https://ollama.example.com";
        assert_eq!(
            base_url_from_chat_url(url),
            "https://ollama.example.com"
        );
    }

    #[test]
    fn test_base_url_from_chat_url_empty() {
        let url = "";
        assert_eq!(
            base_url_from_chat_url(url),
            DEFAULT_OLLAMA_BASE_URL
        );
    }

    #[test]
    fn test_base_url_from_chat_url_only_api_chat() {
        let url = "/api/chat";
        assert_eq!(
            base_url_from_chat_url(url),
            DEFAULT_OLLAMA_BASE_URL
        );
    }

    #[tokio::test]
    async fn test_backend_for_routes_local_prefix_to_local_backend() {
        // A `local:` model string routes to LocalModelBackend for that caller.
        // With no callback registered it yields Unavailable when invoked.
        let store = Arc::new(Mutex::new(PodcastStore::new()));

        let backend = backend_for(&store, "local:gemma4-e2b");
        let req = crate::llm::LlmRequest {
            system: "test".to_string(),
            history: vec![],
            user: "test".to_string(),
            model: "test".to_string(),
        };

        let result = backend.complete(&req).await;
        assert!(matches!(result, Err(crate::llm::LlmError::Unavailable(_))));
    }
}

use serde_json::json;

use crate::runtime::{AppRuntime, Result};

impl AppRuntime {
    pub fn set_agent_initial_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_agent_initial_model", model, model_name)
    }

    pub fn set_agent_thinking_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_agent_thinking_model", model, model_name)
    }

    pub fn set_memory_compilation_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_memory_compilation_model", model, model_name)
    }

    pub fn set_wiki_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_wiki_model", model, model_name)
    }

    pub fn set_categorization_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_categorization_model", model, model_name)
    }

    pub fn set_chapter_compilation_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_chapter_compilation_model", model, model_name)
    }

    pub fn set_embeddings_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_embeddings_model", model, model_name)
    }

    pub fn set_image_generation_model(&self, model: &str, model_name: &str) -> Result<String> {
        self.set_named_model("set_image_generation_model", model, model_name)
    }

    pub fn set_reranker_enabled(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_reranker_enabled", "enabled": enabled}),
        )
    }

    pub fn set_open_router_credential(
        &self,
        source: &str,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_open_router_credential",
                "source": source,
                "key_id": key_id,
                "key_label": key_label,
                "connected_at": connected_at,
            }),
        )
    }

    pub fn set_ollama_credential(
        &self,
        source: &str,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_ollama_credential",
                "source": source,
                "key_id": key_id,
                "key_label": key_label,
                "connected_at": connected_at,
            }),
        )
    }

    pub fn set_ollama_chat_url(&self, url: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_ollama_chat_url", "url": url}),
        )
    }

    pub fn set_eleven_labs_credential(
        &self,
        source: &str,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_eleven_labs_credential",
                "source": source,
                "key_id": key_id,
                "key_label": key_label,
                "connected_at": connected_at,
            }),
        )
    }

    pub fn set_stt_provider(&self, provider: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_stt_provider", "provider": provider}),
        )
    }

    pub fn set_stt_keys_present(&self, providers: Vec<String>) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_stt_keys_present", "providers": providers}),
        )
    }

    pub fn set_open_router_whisper_model(&self, model: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_open_router_whisper_model", "model": model}),
        )
    }

    pub fn set_assembly_ai_stt_model(&self, model: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_assembly_ai_stt_model", "model": model}),
        )
    }

    pub fn set_eleven_labs_models(&self, stt_model: &str, tts_model: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_eleven_labs_models",
                "stt_model": stt_model,
                "tts_model": tts_model,
            }),
        )
    }

    pub fn set_eleven_labs_voice(&self, voice_id: &str, voice_name: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_eleven_labs_voice",
                "voice_id": voice_id,
                "voice_name": voice_name,
            }),
        )
    }

    pub fn set_local_model(&self, model_id: Option<String>) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_local_model", "model_id": model_id}),
        )
    }

    pub fn set_provider_api_keys(
        &self,
        open_router: Option<String>,
        ollama: Option<String>,
        eleven_labs: Option<String>,
    ) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({
                "op": "set_provider_api_keys",
                "open_router": open_router,
                "ollama": ollama,
                "eleven_labs": eleven_labs,
            }),
        )
    }

    fn set_named_model(&self, op: &str, model: &str, model_name: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": op, "model": model, "model_name": model_name}),
        )
    }
}

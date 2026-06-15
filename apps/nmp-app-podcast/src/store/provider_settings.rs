//! Provider and model setting accessors for [`super::PodcastStore`].

use crate::llm::provider_config::normalize_ollama_chat_url;

use super::PodcastStore;

impl PodcastStore {
    /// LLM model ID for initial agent chat. Default "deepseek-v4-flash:cloud".
    pub fn agent_initial_model(&self) -> &str {
        &self.agent_initial_model
    }

    /// Human-readable name for the initial agent model. Default "DeepSeek Flash".
    pub fn agent_initial_model_name(&self) -> &str {
        &self.agent_initial_model_name
    }

    /// Set both the model ID and name for initial agent chat. Idempotent.
    pub fn set_agent_initial_model(&mut self, model: String, model_name: String) {
        if self.agent_initial_model == model && self.agent_initial_model_name == model_name {
            return;
        }
        self.agent_initial_model = model;
        self.agent_initial_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for agent thinking/planning. Default "deepseek-v4-pro:cloud".
    pub fn agent_thinking_model(&self) -> &str {
        &self.agent_thinking_model
    }

    /// Human-readable name for the agent thinking model. Default "DeepSeek Pro".
    pub fn agent_thinking_model_name(&self) -> &str {
        &self.agent_thinking_model_name
    }

    /// Set both the model ID and name for agent thinking/planning. Idempotent.
    pub fn set_agent_thinking_model(&mut self, model: String, model_name: String) {
        if self.agent_thinking_model == model && self.agent_thinking_model_name == model_name {
            return;
        }
        self.agent_thinking_model = model;
        self.agent_thinking_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for memory compilation. Default "deepseek-v4-flash:cloud".
    pub fn memory_compilation_model(&self) -> &str {
        &self.memory_compilation_model
    }

    /// Human-readable name for the memory compilation model. Default "DeepSeek Flash".
    pub fn memory_compilation_model_name(&self) -> &str {
        &self.memory_compilation_model_name
    }

    /// Set both the model ID and name for memory compilation. Idempotent.
    pub fn set_memory_compilation_model(&mut self, model: String, model_name: String) {
        if self.memory_compilation_model == model
            && self.memory_compilation_model_name == model_name
        {
            return;
        }
        self.memory_compilation_model = model;
        self.memory_compilation_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for wiki synthesis. Default "deepseek-v4-flash:cloud".
    pub fn wiki_model(&self) -> &str {
        &self.wiki_model
    }

    /// Human-readable name for the wiki model. Default "DeepSeek Flash".
    pub fn wiki_model_name(&self) -> &str {
        &self.wiki_model_name
    }

    /// Set both the model ID and name for wiki synthesis. Idempotent.
    pub fn set_wiki_model(&mut self, model: String, model_name: String) {
        if self.wiki_model == model && self.wiki_model_name == model_name {
            return;
        }
        self.wiki_model = model;
        self.wiki_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for episode categorization. Default "deepseek-v4-flash:cloud".
    pub fn categorization_model(&self) -> &str {
        &self.categorization_model
    }

    /// Human-readable name for the categorization model. Default "DeepSeek Flash".
    pub fn categorization_model_name(&self) -> &str {
        &self.categorization_model_name
    }

    /// Set both the model ID and name for categorization. Idempotent.
    pub fn set_categorization_model(&mut self, model: String, model_name: String) {
        if self.categorization_model == model && self.categorization_model_name == model_name {
            return;
        }
        self.categorization_model = model;
        self.categorization_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for chapter compilation. Default "deepseek-v4-flash:cloud".
    pub fn chapter_compilation_model(&self) -> &str {
        &self.chapter_compilation_model
    }

    /// Human-readable name for the chapter compilation model. Default "DeepSeek Flash".
    pub fn chapter_compilation_model_name(&self) -> &str {
        &self.chapter_compilation_model_name
    }

    /// Set both the model ID and name for chapter compilation. Idempotent.
    pub fn set_chapter_compilation_model(&mut self, model: String, model_name: String) {
        if self.chapter_compilation_model == model
            && self.chapter_compilation_model_name == model_name
        {
            return;
        }
        self.chapter_compilation_model = model;
        self.chapter_compilation_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for embeddings generation. Default "openai/text-embedding-3-large".
    pub fn embeddings_model(&self) -> &str {
        &self.embeddings_model
    }

    /// Human-readable name for the embeddings model. Default "text-embedding-3-large".
    pub fn embeddings_model_name(&self) -> &str {
        &self.embeddings_model_name
    }

    /// Set both the model ID and name for embeddings. Idempotent.
    pub fn set_embeddings_model(&mut self, model: String, model_name: String) {
        if self.embeddings_model == model && self.embeddings_model_name == model_name {
            return;
        }
        self.embeddings_model = model;
        self.embeddings_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for image generation. Default "google/gemini-2.5-flash-image".
    pub fn image_generation_model(&self) -> &str {
        &self.image_generation_model
    }

    /// Human-readable name for the image generation model. Default "Gemini 2.5 Flash".
    pub fn image_generation_model_name(&self) -> &str {
        &self.image_generation_model_name
    }

    /// Set both the model ID and name for image generation. Idempotent.
    pub fn set_image_generation_model(&mut self, model: String, model_name: String) {
        if self.image_generation_model == model && self.image_generation_model_name == model_name {
            return;
        }
        self.image_generation_model = model;
        self.image_generation_model_name = model_name;
        self.persist();
    }

    /// Whether the reranker is enabled for search results. Default `false`.
    pub fn reranker_enabled(&self) -> bool {
        self.reranker_enabled
    }

    /// Set the reranker-enabled toggle and persist. Idempotent.
    pub fn set_reranker_enabled(&mut self, value: bool) {
        if self.reranker_enabled == value {
            return;
        }
        self.reranker_enabled = value;
        self.persist();
    }

    /// Ollama chat endpoint URL for LLM inference.
    pub fn ollama_chat_url(&self) -> &str {
        &self.ollama_chat_url
    }

    /// Set Ollama chat URL and persist. Idempotent.
    pub fn set_ollama_chat_url(&mut self, url: String) {
        let normalized = normalize_ollama_chat_url(&url);
        if self.ollama_chat_url == normalized {
            return;
        }
        self.ollama_chat_url = normalized;
        self.persist();
    }

    /// STT provider selection (enum .rawValue String). Default `"apple_native"`.
    pub fn stt_provider(&self) -> &str {
        &self.stt_provider
    }

    /// Set the STT provider and persist. Idempotent.
    pub fn set_stt_provider(&mut self, value: String) {
        if self.stt_provider == value {
            return;
        }
        self.stt_provider = value;
        self.persist();
    }

    /// OpenRouter Whisper model string. Default `"openai/whisper-1"`.
    pub fn open_router_whisper_model(&self) -> &str {
        &self.open_router_whisper_model
    }

    /// Set the OpenRouter Whisper model and persist. Idempotent.
    pub fn set_open_router_whisper_model(&mut self, value: String) {
        if self.open_router_whisper_model == value {
            return;
        }
        self.open_router_whisper_model = value;
        self.persist();
    }

    /// AssemblyAI STT model string. Default `"universal-3-pro,universal-2"`.
    pub fn assembly_ai_stt_model(&self) -> &str {
        &self.assembly_ai_stt_model
    }

    /// Set the AssemblyAI STT model and persist. Idempotent.
    pub fn set_assembly_ai_stt_model(&mut self, value: String) {
        if self.assembly_ai_stt_model == value {
            return;
        }
        self.assembly_ai_stt_model = value;
        self.persist();
    }

    /// ElevenLabs STT model string. Default `"scribe_v1"`.
    pub fn eleven_labs_stt_model(&self) -> &str {
        &self.eleven_labs_stt_model
    }

    /// ElevenLabs TTS model string. Default `"eleven_turbo_v2_5"`.
    pub fn eleven_labs_tts_model(&self) -> &str {
        &self.eleven_labs_tts_model
    }

    /// Set both ElevenLabs STT and TTS models and persist. Atomic update.
    pub fn set_eleven_labs_models(&mut self, stt_model: String, tts_model: String) {
        if self.eleven_labs_stt_model == stt_model && self.eleven_labs_tts_model == tts_model {
            return;
        }
        self.eleven_labs_stt_model = stt_model;
        self.eleven_labs_tts_model = tts_model;
        self.persist();
    }

    /// ElevenLabs voice ID. Defaults to empty string.
    pub fn eleven_labs_voice_id(&self) -> &str {
        &self.eleven_labs_voice_id
    }

    /// ElevenLabs voice name. Defaults to empty string.
    pub fn eleven_labs_voice_name(&self) -> &str {
        &self.eleven_labs_voice_name
    }

    /// Set both ElevenLabs voice ID and name and persist. Atomic update.
    pub fn set_eleven_labs_voice(&mut self, voice_id: String, voice_name: String) {
        if self.eleven_labs_voice_id == voice_id && self.eleven_labs_voice_name == voice_name {
            return;
        }
        self.eleven_labs_voice_id = voice_id;
        self.eleven_labs_voice_name = voice_name;
        self.persist();
    }
}

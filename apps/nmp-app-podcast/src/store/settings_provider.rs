//! Provider API key accessors for [`super::PodcastStore`].
//!
//! Covers the in-memory provider credentials (OpenRouter, Ollama, ElevenLabs,
//! AssemblyAI, Perplexity) that are never persisted to disk.

use super::PodcastStore;

impl PodcastStore {
    /// OpenRouter API key (in-memory only; never persisted to disk).
    pub fn open_router_api_key(&self) -> Option<&str> {
        self.open_router_api_key.as_deref()
    }

    /// Ollama API key (in-memory only; never persisted to disk).
    pub fn ollama_api_key(&self) -> Option<&str> {
        self.ollama_api_key.as_deref()
    }

    /// ElevenLabs API key (in-memory only; never persisted to disk).
    pub fn eleven_labs_api_key(&self) -> Option<&str> {
        self.eleven_labs_api_key.as_deref()
    }

    /// AssemblyAI API key (in-memory only; never persisted to disk).
    pub fn assembly_ai_api_key(&self) -> Option<&str> {
        self.assembly_ai_api_key.as_deref()
    }

    /// Perplexity API key (in-memory only; never persisted to disk).
    pub fn perplexity_api_key(&self) -> Option<&str> {
        self.perplexity_api_key.as_deref()
    }

    /// Set provider API keys in-memory. Does NOT call `persist()`; these keys
    /// never touch disk. Idempotent.
    pub fn set_provider_api_keys(
        &mut self,
        open_router: Option<String>,
        ollama: Option<String>,
        eleven_labs: Option<String>,
        assembly_ai: Option<String>,
        perplexity: Option<String>,
    ) {
        self.open_router_api_key = open_router;
        self.ollama_api_key = ollama;
        self.eleven_labs_api_key = eleven_labs;
        self.assembly_ai_api_key = assembly_ai;
        self.perplexity_api_key = perplexity;
    }
}

//! Provider-blind LLM dispatch layer.
//!
//! Abstracts over multiple LLM providers (Ollama, OpenRouter, Local) via a single
//! [`LlmBackend`] trait. Callers select a backend via [`backend_for`] based on
//! the model string and stored credential state, then invoke [`LlmBackend::complete`]
//! for a single async turn.

pub mod assemblyai_transcript;
pub mod backend;
pub mod elevenlabs_key_validation;
pub mod elevenlabs_scribe;
pub mod elevenlabs_tts;
pub mod elevenlabs_voice_catalog;
pub mod factory;
pub mod image_generation;
pub mod local_model_backend;
pub mod local_model_catalog;
pub mod model_catalog;
mod model_catalog_dtos;
pub mod ollama_backend;
pub mod openrouter_backend;
pub mod openrouter_key_validation;
pub mod openrouter_whisper;
mod openrouter_whisper_audio;
pub mod perplexity_search;
pub mod provider_config;
pub mod provider_transport;
pub mod rerank_backend;
pub mod speech_model_catalog;

pub use backend::{LlmBackend, LlmError, LlmRequest};
pub use factory::{backend_for, role_model_or_default, validate_model_credentials};
pub use local_model_backend::LocalModelBackend;
pub use rerank_backend::{rerank_openrouter, RerankError, RerankRequest};

pub fn is_missing_credential_error(error: &str) -> bool {
    error.contains("Missing credential:")
}

//! Provider-blind LLM dispatch layer.
//!
//! Abstracts over multiple LLM providers (Ollama, OpenRouter, Local) via a single
//! [`LlmBackend`] trait. Callers select a backend via [`backend_for`] based on
//! the model string and stored credential state, then invoke [`LlmBackend::complete`]
//! for a single async turn.

pub mod backend;
pub mod factory;
pub mod image_generation;
pub mod local_model_backend;
pub mod ollama_backend;
pub mod openrouter_backend;
pub mod provider_transport;

pub use backend::{LlmBackend, LlmError, LlmRequest};
pub use factory::{backend_for, role_model_or_default, validate_model_credentials};
pub use local_model_backend::LocalModelBackend;

pub fn is_missing_credential_error(error: &str) -> bool {
    error.contains("Missing credential:")
}

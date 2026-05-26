//! LLM integration for agent chat — thin synchronous wrapper over rig-core + Ollama.
//!
//! The sole exported function [`chat_sync`] drives a blocking Ollama call from the
//! actor thread (which is a plain `std::thread`, not a Tokio worker). The Tokio
//! runtime is passed in from [`super::host_op_handler::PodcastHostOpHandler`] so
//! the caller can reuse the shared multi-thread scheduler rather than spinning up
//! a new one per call.
//!
//! Model selection follows AGENTS.md:
//! - [`THINKING_MODEL`] for agent-chat turns (reasoning mode).
//! - [`FAST_MODEL`] is the fallback when the primary model is unavailable.

use rig_core::client::{CompletionClient, Nothing};
use rig_core::completion::{Chat, Message};
use rig_core::providers::ollama;

/// Fast, low-latency model for iterative requests.
pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";

/// Thinking/agent model for deep-reasoning chat turns.
pub const THINKING_MODEL: &str = "deepseek-v4-pro:cloud";

/// Ollama base URL used across all LLM requests in the app.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Drive a single-turn chat call synchronously.
///
/// `history` contains the conversation **up to but not including** the new user
/// message. The caller is responsible for building this slice from the in-memory
/// transcript; this function does not own or mutate the stored conversation.
///
/// Returns the assistant reply string on success. Returns `Err(reason)` if Ollama
/// is unreachable, the model refuses, or any other transport/protocol error fires.
///
/// # Panics
/// Does not panic. The caller falls back to the scaffold reply on any `Err`.
pub fn chat_sync(
    system_prompt: &str,
    history: &[(String, String)],  // (role, content) pairs before the new user turn
    user_message: &str,
    runtime: &tokio::runtime::Runtime,
) -> Result<String, String> {
    runtime.block_on(async {
        let client = ollama::Client::builder()
            .base_url(OLLAMA_BASE_URL)
            .api_key(Nothing)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = client
            .agent(THINKING_MODEL)
            .preamble(system_prompt)
            .build();

        // Build the rig-core chat history from our (role, content) slice.
        // The Chat trait will prepend the user turn internally — we only pass prior turns.
        let mut chat_history: Vec<Message> = history
            .iter()
            .map(|(role, content)| {
                if role == "user" {
                    Message::user(content.as_str())
                } else {
                    Message::assistant(content.as_str())
                }
            })
            .collect();

        agent
            .chat(user_message, &mut chat_history)
            .await
            .map_err(|e| e.to_string())
    })
}

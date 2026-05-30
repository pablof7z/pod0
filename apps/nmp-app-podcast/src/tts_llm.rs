//! LLM-based TTS-script generation using rig-core + Ollama.
//!
//! [`generate_tts_script`] takes a topic and a target length and asks a local
//! Ollama instance to write a single-voice podcast monologue script. The reply
//! is returned verbatim (trimmed) as the script the iOS voice executor will
//! speak when the episode is played.
//!
//! The function is synchronous at the call site — the caller supplies the
//! shared Tokio runtime and we `block_on` so the actor thread (or a
//! `spawn_blocking` worker) can drive it without being async itself. This
//! mirrors [`crate::briefing_llm::generate_briefing_segments`].
//!
//! ## Failure handling
//!
//! Returns `Err(String)` when Ollama is unreachable or the model reply is
//! empty / whitespace-only. The caller
//! ([`crate::tts::TtsEpisodeHandler::handle_generate`]) catches the error and
//! falls back to [`crate::tts::placeholder_script`] so the episode always ends
//! up with a non-empty script instead of staying stuck on the
//! "Generating script…" placeholder.

use std::sync::Arc;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::{Prompt as _, PromptError};
use rig_core::providers::ollama;

pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Generate a single-voice podcast monologue script for `topic`.
///
/// `length_minutes` shapes the system prompt so the model targets a runtime
/// roughly matching the user's request; it is not a hard cap on output.
///
/// Returns the trimmed script on success, or `Err(message)` when Ollama is
/// unreachable or the reply is empty / whitespace-only. The caller falls back
/// to [`crate::tts::placeholder_script`] on `Err`.
pub fn generate_tts_script(
    topic: &str,
    length_minutes: u32,
    runtime: &Arc<tokio::runtime::Runtime>,
) -> Result<String, String> {
    let preamble = build_preamble(length_minutes, topic);
    let prompt = format!("Topic: {topic}\nLength: {length_minutes} minutes");

    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = client.agent(FAST_MODEL).preamble(&preamble).build();

        let response: String = agent
            .prompt(prompt.as_str())
            .await
            .map_err(|e: PromptError| e.to_string())?;

        parse_script(&response)
    })
}

/// Build the system preamble for the script model. Kept separate so the
/// `{length_minutes}` / `{topic}` interpolation is testable without a live
/// model.
fn build_preamble(length_minutes: u32, topic: &str) -> String {
    format!(
        "You are a podcast script writer. Write an engaging {length_minutes}-minute podcast \
         monologue script about: {topic}. Write in a conversational, enthusiastic tone. Output \
         ONLY the script text, no stage directions, no headers."
    )
}

/// Validate + normalise an LLM script reply.
///
/// Trims surrounding whitespace and returns the script on success, or `Err`
/// when the reply is empty / whitespace-only — the condition the call site
/// treats as "the model gave us nothing usable" and falls back to the
/// placeholder. Pure so the two required tests don't need a live Ollama.
pub(crate) fn parse_script(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("LLM returned an empty TTS script".to_owned());
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
#[path = "tts_llm_tests.rs"]
mod tests;

//! LLM synthesis for wiki articles using rig-core + Ollama.
//!
//! All I/O is `async`; callers on the actor thread use `runtime.block_on`
//! so the actor stays single-threaded while the Tokio scheduler drives the
//! HTTP round-trip to `localhost:11434`.
//!
//! ## Failure contract
//!
//! Returns `Err(String)` when Ollama is unreachable or returns an error.
//! The caller (`wiki::handle_generate`) stores the error string on
//! `WikiArticle::generation_error` so the UI can surface it as a retry
//! banner; the article itself is committed with the placeholder summary so
//! the user keeps a readable (if stale) record.

use rig_core::client::{CompletionClient, Nothing};
use rig_core::completion::{Prompt as _, PromptError};
use rig_core::providers::ollama;

pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Synthesize a wiki summary from the topic and transcript excerpts.
///
/// Returns the LLM-generated body string on success, or `Err(message)` when
/// Ollama is unavailable or returns a non-200 response.
///
/// `transcript_excerpts` will be truncated to ~4 000 chars total before
/// being injected into the prompt so the context window stays bounded even
/// when many episodes have long transcripts.
pub fn synthesize_summary(
    topic: &str,
    podcast_title: &str,
    transcript_excerpts: &[String],
    runtime: &tokio::runtime::Runtime,
) -> Result<String, String> {
    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = client
            .agent(FAST_MODEL)
            .preamble(
                "You are a research assistant writing concise, factual wiki articles \
                 about podcast topics. Write 2-3 paragraphs, no headers, no markdown.",
            )
            .build();

        // Truncate transcripts to ~4 000 chars total to keep prompt size bounded.
        let context: String = transcript_excerpts
            .iter()
            .flat_map(|t| t.chars())
            .take(4_000)
            .collect();

        let prompt = if context.is_empty() {
            format!(
                "Write a wiki article about '{topic}' as discussed on the podcast '{podcast_title}'.",
            )
        } else {
            format!(
                "Write a wiki article about '{topic}' as discussed on the podcast \
                 '{podcast_title}'. Use these transcript excerpts as source material:\n\n{context}",
            )
        };

        agent
            .prompt(prompt.as_str())
            .await
            .map_err(|e: PromptError| e.to_string())
    })
}

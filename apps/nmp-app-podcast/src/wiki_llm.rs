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

use std::sync::{Arc, Mutex};

use crate::llm::{LlmRequest, backend_for, role_model_or_default};
use crate::store::PodcastStore;

pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";

/// Synthesize a wiki summary from the topic and transcript excerpts.
///
/// Returns the LLM-generated body string on success, or `Err(message)` when
/// the LLM is unavailable or returns an error.
///
/// `transcript_excerpts` will be truncated to ~4 000 chars total before
/// being injected into the prompt so the context window stays bounded even
/// when many episodes have long transcripts.
///
/// `context_chunks` are RAG hits from the knowledge store (M5.6-wiki):
/// transcript chunks the ranker found relevant to `topic`. They're injected
/// as a separate "Related knowledge chunks:" section, capped at ~2 000 chars
/// so they augment — rather than crowd out — the broader transcript context.
pub fn synthesize_summary(
    topic: &str,
    podcast_title: &str,
    transcript_excerpts: &[String],
    context_chunks: &[String],
    runtime: &tokio::runtime::Runtime,
    store: &Arc<Mutex<PodcastStore>>,
) -> Result<String, String> {
    runtime.block_on(async {
        // Truncate transcripts to ~4 000 chars total to keep prompt size bounded.
        let context: String = transcript_excerpts
            .iter()
            .flat_map(|t| t.chars())
            .take(4_000)
            .collect();

        // RAG chunks join into their own section, capped at ~2 000 chars so
        // they don't blow the context window when many chunks match.
        let chunks_context: String = context_chunks
            .iter()
            .flat_map(|c| c.chars().chain(std::iter::once('\n')))
            .take(2_000)
            .collect();

        let mut prompt = format!(
            "Write a wiki article about '{topic}' as discussed on the podcast '{podcast_title}'.",
        );
        if !context.is_empty() {
            prompt.push_str(&format!(
                " Use these transcript excerpts as source material:\n\n{context}",
            ));
        }
        if !chunks_context.is_empty() {
            prompt.push_str(&format!(
                "\n\nRelated knowledge chunks:\n\n{chunks_context}",
            ));
        }

        // Honor a `local:` selection for the Wiki role; otherwise the cloud
        // fast model, unchanged.
        let wiki_cfg = store
            .lock()
            .ok()
            .map(|s| s.wiki_model().to_owned())
            .unwrap_or_default();
        let wiki_model = role_model_or_default(&wiki_cfg, FAST_MODEL);
        let backend = backend_for(store, &wiki_model);
        let req = LlmRequest {
            system: "You are a research assistant writing concise, factual wiki articles \
                     about podcast topics. Write 2-3 paragraphs, no headers, no markdown."
                .to_owned(),
            history: Vec::new(),
            user: prompt,
            model: wiki_model.clone(),
        };

        backend.complete(&req).await.map_err(|e| e.to_string())
    })
}

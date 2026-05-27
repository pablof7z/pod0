//! AI-wiki action handlers used by
//! `PodcastHostOpHandler::handle_wiki_action`.
//!
//! Factored out of `host_op_handler.rs` so that file stays inside the
//! 500-line hard limit (AGENTS.md). The functions here are deliberately
//! free functions that take `Arc<Mutex<…>>` slots so they're trivially
//! reusable from the actor thread without inheriting the handler's
//! capability-dispatch context.
//!
//! ## LLM synthesis (PR 4 — wiki-llm)
//!
//! `handle_generate` calls `wiki_llm::synthesize_summary` via
//! `runtime.block_on` to fetch a real summary from Ollama. On success the
//! placeholder is replaced with the LLM body. On failure (Ollama offline,
//! model error) the placeholder is kept and `generation_error` is set so
//! the iOS shell can surface a retry banner.
//!
//! Every handler is fire-and-forget per D6: lock poisoning degrades to
//! `{"ok":false,"error":"…"}` rather than panicking across the FFI.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use crate::ffi::actions::wiki_module::WikiAction;
use crate::ffi::projections::WikiArticle;
use crate::store::PodcastStore;
use crate::wiki_llm;

/// Dispatch a [`WikiAction`] against the wiki slots on the handle and
/// bump `rev` on any state change.
///
/// The returned envelope is the JSON the action substrate forwards back
/// to the iOS dispatcher.
pub(crate) fn handle_wiki_action(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    search_results: &Arc<Mutex<Vec<WikiArticle>>>,
    store: &Arc<Mutex<PodcastStore>>,
    rev: &AtomicU64,
    runtime: &Arc<Runtime>,
    action: WikiAction,
) -> serde_json::Value {
    match action {
        WikiAction::Generate { podcast_id, topic } => {
            handle_generate(articles, store, rev, runtime, podcast_id, topic)
        }
        WikiAction::Delete { article_id } => {
            handle_delete(articles, search_results, rev, article_id)
        }
        WikiAction::Search { query } => handle_search(articles, search_results, rev, query),
    }
}

fn handle_generate(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    store: &Arc<Mutex<PodcastStore>>,
    rev: &AtomicU64,
    runtime: &Arc<Runtime>,
    podcast_id: String,
    topic: String,
) -> serde_json::Value {
    let topic_trimmed = topic.trim();
    if topic_trimmed.is_empty() {
        return serde_json::json!({"ok": false, "error": "topic is empty"});
    }
    if podcast_id.trim().is_empty() {
        return serde_json::json!({"ok": false, "error": "podcast_id is empty"});
    }

    // Collect podcast title + stored transcripts for LLM context.
    let (podcast_title, transcripts) = {
        match store.lock() {
            Ok(s) => {
                use podcast_core::PodcastId;
                use uuid::Uuid;
                let pid = Uuid::parse_str(&podcast_id)
                    .ok()
                    .map(PodcastId::new);
                let title = pid
                    .and_then(|id| s.podcast(id))
                    .map(|p| p.title.clone())
                    .unwrap_or_else(|| podcast_id.clone());
                // Collect all episode ids for this podcast, then look up
                // each transcript from the store's in-memory cache.
                let eps = pid
                    .map(|id| s.episodes_for(id).to_vec())
                    .unwrap_or_default();
                let txs: Vec<String> = eps
                    .iter()
                    .filter_map(|ep| {
                        s.transcript_for(&ep.id.0.to_string())
                            .map(|t| t.to_owned())
                    })
                    .filter(|t| !t.is_empty())
                    .collect();
                (title, txs)
            }
            Err(_) => (podcast_id.clone(), Vec::new()),
        }
    };

    let article_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    // Attempt LLM synthesis; fall back to placeholder on error.
    let placeholder = format!(
        "This article about {topic} will be generated from episode transcripts and \
         web research. LLM synthesis is a follow-up.",
        topic = topic_trimmed
    );
    let (summary, generation_error) =
        match wiki_llm::synthesize_summary(topic_trimmed, &podcast_title, &transcripts, runtime) {
            Ok(body) => (body, None),
            Err(e) => (placeholder, Some(e)),
        };

    let article = WikiArticle {
        id: article_id.clone(),
        podcast_id,
        topic: topic_trimmed.to_owned(),
        summary,
        source_episode_ids: Vec::new(),
        last_updated_at: now,
        is_generating: false,
        generation_error,
    };
    match articles.lock() {
        Ok(mut w) => {
            w.push(article);
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true, "article_id": article_id})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "wiki_articles poisoned"}),
    }
}

fn handle_delete(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    search_results: &Arc<Mutex<Vec<WikiArticle>>>,
    rev: &AtomicU64,
    article_id: String,
) -> serde_json::Value {
    let removed = match articles.lock() {
        Ok(mut w) => {
            let before = w.len();
            w.retain(|a| a.id != article_id);
            before != w.len()
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "wiki_articles poisoned"}),
    };
    // Also drop the article from any active search result so the UI
    // doesn't show a stale row pointing at a deleted id.
    if let Ok(mut s) = search_results.lock() {
        s.retain(|a| a.id != article_id);
    }
    if removed {
        rev.fetch_add(1, Ordering::Relaxed);
    }
    serde_json::json!({"ok": true})
}

fn handle_search(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    search_results: &Arc<Mutex<Vec<WikiArticle>>>,
    rev: &AtomicU64,
    query: String,
) -> serde_json::Value {
    let q = query.trim().to_lowercase();
    let snapshot = match articles.lock() {
        Ok(w) => w.clone(),
        Err(_) => return serde_json::json!({"ok": false, "error": "wiki_articles poisoned"}),
    };
    let results: Vec<WikiArticle> = if q.is_empty() {
        // Empty query clears the search overlay.
        Vec::new()
    } else {
        snapshot
            .into_iter()
            .filter(|a| a.topic.to_lowercase().contains(&q))
            .collect()
    };
    match search_results.lock() {
        Ok(mut s) => {
            *s = results;
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "wiki_search_results poisoned"}),
    }
}

#[cfg(test)]
#[path = "wiki_tests.rs"]
mod tests;

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

use podcast_knowledge::KnowledgeStore;

use crate::ffi::actions::wiki_module::WikiAction;
use crate::ffi::projections::WikiArticle;
use crate::knowledge::collect_chunk_texts_for_topic;
use crate::store::PodcastStore;
use crate::wiki_llm;

/// RAG context chunks pulled into the wiki prompt per generate (M5.6-wiki).
const WIKI_CONTEXT_CHUNK_LIMIT: usize = 5;

/// Dispatch a [`WikiAction`] against the wiki slots on the handle and
/// bump `rev` on any state change.
///
/// The returned envelope is the JSON the action substrate forwards back
/// to the iOS dispatcher.
pub(crate) fn handle_wiki_action(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    search_results: &Arc<Mutex<Vec<WikiArticle>>>,
    store: &Arc<Mutex<PodcastStore>>,
    knowledge_store: &Arc<Mutex<KnowledgeStore>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    action: WikiAction,
) -> serde_json::Value {
    match action {
        WikiAction::Generate { podcast_id, topic } => {
            handle_generate(articles, store, knowledge_store, rev, runtime, podcast_id, topic)
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
    knowledge_store: &Arc<Mutex<KnowledgeStore>>,
    rev: &Arc<AtomicU64>,
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

    // Collect podcast title + stored transcripts for LLM context, plus the
    // episode ids so the RAG chunk search below stays scoped to this podcast.
    let (podcast_title, transcripts, episode_ids) = {
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
                let ep_ids: Vec<String> =
                    eps.iter().map(|ep| ep.id.0.to_string()).collect();
                let txs: Vec<String> = eps
                    .iter()
                    .filter_map(|ep| {
                        s.transcript_for(&ep.id.0.to_string())
                            .map(|t| t.to_owned())
                    })
                    .filter(|t| !t.is_empty())
                    .collect();
                (title, txs, ep_ids)
            }
            Err(_) => (podcast_id.clone(), Vec::new(), Vec::new()),
        }
    };

    // M5.6-wiki: pull the most relevant indexed transcript chunks for the
    // topic so the LLM gets focused RAG context alongside the broad
    // transcripts. Scoped to this podcast's episodes — the knowledge store is
    // global, but a per-podcast article must not cite an unrelated podcast.
    // Collected synchronously (lock dropped before spawn), mirroring the
    // transcript collection above. Each hit carries its owning episode id so
    // the article can record which episodes it drew from (M9 attribution).
    let chunk_hits: Vec<(String, String)> = match knowledge_store.lock() {
        Ok(ks) => collect_chunk_texts_for_topic(
            &ks,
            topic_trimmed,
            &episode_ids,
            WIKI_CONTEXT_CHUNK_LIMIT,
        ),
        Err(_) => Vec::new(),
    };

    // M9 source attribution: the source episodes are exactly those whose
    // chunks entered the LLM context window (the truncated top-N hits), not
    // the broad per-podcast scope. Deduped and sorted for snapshot stability.
    let source_episode_ids: Vec<String> = {
        let mut ids: Vec<String> =
            chunk_hits.iter().map(|(ep, _)| ep.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let context_chunks: Vec<String> =
        chunk_hits.into_iter().map(|(_, text)| text).collect();

    let article_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    // M5.2: insert a placeholder article immediately (is_generating=true) so
    // the iOS snapshot shows the article card while synthesis runs off-thread.
    let placeholder_summary = format!(
        "Generating an article about '{topic}'…",
        topic = topic_trimmed
    );
    let placeholder_article = WikiArticle {
        id: article_id.clone(),
        podcast_id: podcast_id.clone(),
        topic: topic_trimmed.to_owned(),
        summary: placeholder_summary,
        source_episode_ids,
        last_updated_at: now,
        is_generating: true,
        generation_error: None,
    };
    match articles.lock() {
        Ok(mut w) => w.push(placeholder_article),
        Err(_) => return serde_json::json!({"ok": false, "error": "wiki_articles poisoned"}),
    }
    rev.fetch_add(1, Ordering::Relaxed);

    // Spawn synthesis off the actor thread. When done, find and update the
    // placeholder in-place so the iOS snapshot surfaces the real article.
    let articles_c = Arc::clone(articles);
    let runtime_c = Arc::clone(runtime);
    let rev_c = Arc::clone(rev);
    let article_id_c = article_id.clone();
    let topic_owned = topic_trimmed.to_owned();
    let placeholder_fallback = format!(
        "Could not generate an article about '{topic_trimmed}'. Check that Ollama is running."
    );

    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            wiki_llm::synthesize_summary(
                &topic_owned,
                &podcast_title,
                &transcripts,
                &context_chunks,
                &runtime_c,
            )
        })
        .await;

        let (summary, error) = match result {
            Ok(Ok(body)) => (body, None),
            Ok(Err(e)) => (placeholder_fallback, Some(e)),
            Err(_) => (placeholder_fallback, Some("synthesis task panicked".to_owned())),
        };

        if let Ok(mut w) = articles_c.lock() {
            if let Some(a) = w.iter_mut().find(|a| a.id == article_id_c) {
                a.summary = summary;
                a.is_generating = false;
                a.generation_error = error;
                a.last_updated_at = Utc::now().timestamp();
            }
        }
        rev_c.fetch_add(1, Ordering::Relaxed);
    });

    serde_json::json!({"ok": true, "article_id": article_id})
}

fn handle_delete(
    articles: &Arc<Mutex<Vec<WikiArticle>>>,
    search_results: &Arc<Mutex<Vec<WikiArticle>>>,
    rev: &Arc<AtomicU64>,
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
    rev: &Arc<AtomicU64>,
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

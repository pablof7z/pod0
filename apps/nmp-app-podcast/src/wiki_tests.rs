//! Tests for [`super::wiki`] — WikiArticle CRUD and search coverage.
//!
//! Extracted from `wiki.rs` to keep that file under the 500-line hard limit.

use std::sync::Arc;
use tokio::runtime::Runtime;

use super::*;

fn make_slots() -> (
    Arc<Mutex<Vec<WikiArticle>>>,
    Arc<Mutex<Vec<WikiArticle>>>,
    Arc<Mutex<crate::store::PodcastStore>>,
    Arc<Mutex<podcast_knowledge::KnowledgeStore>>,
    Arc<AtomicU64>,
    Arc<Runtime>,
) {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    (
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(crate::store::PodcastStore::new())),
        Arc::new(Mutex::new(podcast_knowledge::KnowledgeStore::new())),
        Arc::new(AtomicU64::new(0)),
        rt,
    )
}

/// Tests that Generate inserts a placeholder article synchronously (is_generating=true)
/// and primes rev. The background synthesis task runs off-thread on the production
/// multi-thread runtime; on the test's current-thread runtime the spawn is queued
/// but not polled. End-to-end coverage (is_generating→false, final summary) is
/// deferred to BACKLOG: wiki-generate-e2e-test.
#[test]
fn generate_inserts_placeholder_and_primes_rev() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Bitcoin halvings".into(),
        },
    );
    assert_eq!(envelope["ok"], true);
    let article_id = envelope["article_id"].as_str().unwrap().to_owned();
    assert!(!article_id.is_empty());

    let stored = articles.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].topic, "Bitcoin halvings");
    assert_eq!(stored[0].podcast_id, "pod-1");
    // Placeholder is inserted with is_generating=true; background task fills it.
    assert!(stored[0].is_generating, "placeholder must be is_generating=true");
    assert!(!stored[0].summary.is_empty(), "placeholder summary must not be empty");
    // Exactly one synchronous rev prime before the background task runs.
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn generate_rejects_empty_topic() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "   ".into(),
        },
    );
    assert_eq!(envelope["ok"], false);
    assert!(articles.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn generate_rejects_empty_podcast_id() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "".into(),
            topic: "Topic".into(),
        },
    );
    assert_eq!(envelope["ok"], false);
    assert!(articles.lock().unwrap().is_empty());
}

#[test]
fn delete_removes_article_and_clears_search_row() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Topic".into(),
        },
    );
    let article_id = envelope["article_id"].as_str().unwrap().to_owned();
    // Populate search results with the article so we can prove the
    // delete cascades into the search slot.
    {
        let snap = articles.lock().unwrap().clone();
        *results.lock().unwrap() = snap;
    }
    let rev_before = rev.load(Ordering::Relaxed);
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Delete {
            article_id: article_id.clone(),
        },
    );
    assert_eq!(envelope["ok"], true);
    assert!(articles.lock().unwrap().is_empty());
    assert!(results.lock().unwrap().is_empty());
    assert!(rev.load(Ordering::Relaxed) > rev_before);
}

#[test]
fn delete_unknown_id_does_not_bump_rev() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    let rev_before = rev.load(Ordering::Relaxed);
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Delete {
            article_id: "does-not-exist".into(),
        },
    );
    assert_eq!(envelope["ok"], true);
    assert_eq!(rev.load(Ordering::Relaxed), rev_before);
}

#[test]
fn search_filters_by_topic_substring_case_insensitive() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Bitcoin Halvings".into(),
        },
    );
    handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Lightning Network".into(),
        },
    );
    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Search {
            query: "lightning".into(),
        },
    );
    assert_eq!(envelope["ok"], true);
    let hits = results.lock().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].topic, "Lightning Network");
}

#[test]
fn search_with_empty_query_clears_results() {
    let (articles, results, store, knowledge_store, rev, rt) = make_slots();
    handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Topic".into(),
        },
    );
    handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Search { query: "to".into() },
    );
    assert_eq!(results.lock().unwrap().len(), 1);
    handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Search { query: "  ".into() },
    );
    assert!(results.lock().unwrap().is_empty());
}

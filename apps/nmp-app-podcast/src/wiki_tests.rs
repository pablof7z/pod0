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

/// M9 source attribution: when the topic's RAG chunks come from two
/// episodes, the generated article records both episode ids in
/// `source_episode_ids`. The per-podcast episode scope is derived from the
/// store (so the podcast_id must be a real UUID), and the chunk store is
/// seeded with matching chunks under each episode.
#[test]
fn generate_records_source_episode_ids_from_matched_chunks() {
    use podcast_core::{Episode, Podcast};
    use podcast_knowledge::KnowledgeChunk;
    use podcast_transcripts::TranscriptChunk;

    let (articles, results, store, knowledge_store, rev, rt) = make_slots();

    // Subscribe a real podcast with two episodes so the chunk scope (derived
    // from `episodes_for`) is non-empty. Episode ids are random UUIDs.
    let podcast = Podcast::new("Bitcoin Show");
    let podcast_id = podcast.id;
    let ep1 = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-1",
        "Episode One",
        url::Url::parse("https://example.com/1.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let ep2 = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-2",
        "Episode Two",
        url::Url::parse("https://example.com/2.mp3").unwrap(),
        chrono::Utc::now(),
    );
    // ep3 is in scope but its only chunk shares no tokens with the topic, so
    // it must NOT appear in source_episode_ids — proving attribution tracks
    // contributing episodes, not every episode that happens to be indexed.
    let ep3 = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-3",
        "Episode Three",
        url::Url::parse("https://example.com/3.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let ep1_id = ep1.id.0.to_string();
    let ep2_id = ep2.id.0.to_string();
    let ep3_id = ep3.id.0.to_string();
    store
        .lock()
        .unwrap()
        .subscribe(podcast, vec![ep1, ep2, ep3]);

    // Seed one matching chunk per topic episode (BM25 ranks on the shared
    // "halving" token). ep3's chunk shares no token with the topic — BM25's
    // strictly-positive-score filter must drop it, so ep3 is not attributed.
    {
        let mut ks = knowledge_store.lock().unwrap();
        ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: ep1_id.clone(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 0.0,
            text: "deep dive on the bitcoin halving schedule".to_owned(),
            word_count: 6,
        }));
        ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: ep2_id.clone(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 0.0,
            text: "more on the bitcoin halving and supply".to_owned(),
            word_count: 6,
        }));
        ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: ep3_id.clone(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 0.0,
            text: "unrelated lightning network routing".to_owned(),
            word_count: 4,
        }));
    }

    let envelope = handle_wiki_action(
        &articles,
        &results,
        &store,
        &knowledge_store,
        &rev,
        &rt,
        WikiAction::Generate {
            podcast_id: podcast_id.0.to_string(),
            topic: "halving".into(),
        },
    );
    assert_eq!(envelope["ok"], true);

    let stored = articles.lock().unwrap();
    assert_eq!(stored.len(), 1);
    let sources = &stored[0].source_episode_ids;
    assert_eq!(
        sources.len(),
        2,
        "article must record both contributing episodes, got {sources:?}"
    );
    assert!(sources.contains(&ep1_id), "missing ep1 attribution");
    assert!(sources.contains(&ep2_id), "missing ep2 attribution");
    assert!(
        !sources.contains(&ep3_id),
        "ep3 has no topic-matching chunk and must not be attributed"
    );
    // Deduped + sorted for snapshot stability.
    let mut expected = vec![ep1_id, ep2_id];
    expected.sort();
    assert_eq!(sources, &expected, "source ids must be sorted and deduped");
}

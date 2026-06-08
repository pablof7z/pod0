//! Tests for [`super::knowledge`] — `IndexEpisode` transcript chunking, chunk
//! search, and `collect_chunk_texts_for_topic` scoping (feature M5.3 + RAG).
//!
//! Split out of `knowledge_tests.rs` (which kept the metadata-search matching,
//! ranking, snippet, and snapshot-projection tests) so both files stay under
//! the 500-line hard limit.

use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

fn make_episode(podcast_id: PodcastId, title: &str, description: &str) -> Episode {
    let mut ep = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    ep.description = description.to_owned();
    ep
}

// ── M5.3: IndexEpisode + chunk search ────────────────────────────────

fn shared(store: PodcastStore) -> Arc<Mutex<PodcastStore>> {
    Arc::new(Mutex::new(store))
}

fn empty_knowledge() -> Arc<Mutex<KnowledgeStore>> {
    Arc::new(Mutex::new(KnowledgeStore::new()))
}

#[test]
fn index_episode_without_transcript_reports_no_transcript() {
    let store = shared(PodcastStore::new());
    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    let out = handle_index_episode("missing-ep".to_owned(), &store, &ks, &rev);
    assert_eq!(out["ok"], true);
    assert_eq!(out["status"], "no_transcript");
    assert!(ks.lock().unwrap().is_empty());
}

#[test]
fn index_episode_chunks_stored_transcript() {
    let store = PodcastStore::new();
    let store = shared(store);
    // 450 words → 3 chunks at 200/200/50.
    let text = (0..450)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    store
        .lock()
        .unwrap()
        .set_transcript("ep-1".to_owned(), text);

    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    let before = rev.load(Ordering::Relaxed);
    let out = handle_index_episode("ep-1".to_owned(), &store, &ks, &rev);

    assert_eq!(out["status"], "indexed");
    assert_eq!(out["chunk_count"], 3);
    assert_eq!(ks.lock().unwrap().len(), 3);
    assert!(rev.load(Ordering::Relaxed) > before);

    // chunk_index is sequential and timing is the 0.0 placeholder.
    let guard = ks.lock().unwrap();
    let indices: Vec<u32> = guard.chunks.iter().map(|c| c.chunk.chunk_index).collect();
    assert_eq!(indices, vec![0, 1, 2]);
    assert!(guard.chunks.iter().all(|c| c.chunk.start_secs == 0.0));
    assert!(guard.chunks.iter().all(|c| c.embedding.is_none()));
}

#[test]
fn reindex_same_transcript_does_not_duplicate_chunks() {
    let store = shared(PodcastStore::new());
    let text = "alpha beta gamma".to_owned();
    store
        .lock()
        .unwrap()
        .set_transcript("ep-1".to_owned(), text);
    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    handle_index_episode("ep-1".to_owned(), &store, &ks, &rev);
    handle_index_episode("ep-1".to_owned(), &store, &ks, &rev);
    // delete_episode clears the prior batch before upserting; same transcript
    // → same chunk count, not doubled.
    assert_eq!(ks.lock().unwrap().len(), 1);
}

#[test]
fn reindex_shorter_transcript_removes_stale_trailing_chunks() {
    let store = shared(PodcastStore::new());
    // Synthesize a long first transcript that produces ≥2 chunks.
    let long_text = "word ".repeat(500); // ~2500 chars → ~2-3 chunks
    store
        .lock()
        .unwrap()
        .set_transcript("ep-2".to_owned(), long_text);
    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    handle_index_episode("ep-2".to_owned(), &store, &ks, &rev);
    let first_count = ks.lock().unwrap().len();
    assert!(first_count >= 2, "expected ≥2 chunks from long transcript");

    // Now replace with a short transcript that fits in one chunk.
    store
        .lock()
        .unwrap()
        .set_transcript("ep-2".to_owned(), "short".to_owned());
    handle_index_episode("ep-2".to_owned(), &store, &ks, &rev);
    let second_count = ks.lock().unwrap().len();
    // Stale trailing chunks must be gone — only the new single chunk remains.
    assert_eq!(
        second_count, 1,
        "reindex must clear stale trailing chunks (got {second_count}, had {first_count})"
    );
}

#[test]
fn search_finds_term_only_in_transcript_chunk() {
    let mut raw = PodcastStore::new();
    let podcast = Podcast::new("Tech Talk");
    let id = podcast.id;
    // Title/description deliberately do NOT contain the needle.
    let ep = make_episode(id, "Pilot", "an introductory chat");
    let ep_id = ep.id.0.to_string();
    raw.subscribe(podcast, vec![ep]);
    let store = shared(raw);

    store.lock().unwrap().set_transcript(
        ep_id.clone(),
        "we explore distributed consensus protocols".to_owned(),
    );

    let ks = empty_knowledge();
    let slot = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(1));

    handle_index_episode(ep_id.clone(), &store, &ks, &rev);
    let out = handle_search("consensus".to_owned(), &store, &slot, &ks, &rev);
    assert_eq!(out["ok"], true);

    let results = slot.lock().unwrap();
    assert_eq!(
        results.len(),
        1,
        "transcript-only term must be found via chunk search"
    );
    assert_eq!(results[0].episode_id, ep_id);
    assert_eq!(results[0].episode_title, "Pilot");
    assert_eq!(results[0].podcast_title, "Tech Talk");
    assert!(results[0].snippet.to_lowercase().contains("consensus"));
    // Chunk hits carry a (placeholder) timestamp slot.
    assert_eq!(results[0].start_secs, Some(0.0));
}

#[test]
fn chunk_match_dedups_with_title_match_for_same_episode() {
    let mut raw = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    // Needle "nostr" in BOTH the title and the transcript.
    let ep = make_episode(id, "nostr deep dive", "desc");
    let ep_id = ep.id.0.to_string();
    raw.subscribe(podcast, vec![ep]);
    let store = shared(raw);
    store
        .lock()
        .unwrap()
        .set_transcript(ep_id.clone(), "nostr relays and events".to_owned());

    let ks = empty_knowledge();
    let slot = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(1));
    handle_index_episode(ep_id.clone(), &store, &ks, &rev);
    handle_search("nostr".to_owned(), &store, &slot, &ks, &rev);

    let results = slot.lock().unwrap();
    // One episode → exactly one row; the chunk hit wins (has a timestamp).
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].start_secs, Some(0.0));
}

#[test]
fn chunk_text_for_unknown_episode_is_skipped() {
    // Index a chunk for an episode that isn't in the library → its labels
    // can't be resolved, so the chunk match is dropped.
    let store = shared(PodcastStore::new());
    let ks = empty_knowledge();
    {
        let mut guard = ks.lock().unwrap();
        guard.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: "ghost".to_owned(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 0.0,
            text: "orphaned quantum chunk".to_owned(),
            word_count: 3,
        }));
    }
    let slot = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(1));
    handle_search("quantum".to_owned(), &store, &slot, &ks, &rev);
    assert!(slot.lock().unwrap().is_empty());
}

#[test]
fn empty_transcript_indexes_zero_chunks() {
    let store = shared(PodcastStore::new());
    store
        .lock()
        .unwrap()
        .set_transcript("ep-1".to_owned(), "   ".to_owned());
    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    let out = handle_index_episode("ep-1".to_owned(), &store, &ks, &rev);
    assert_eq!(out["status"], "indexed");
    assert_eq!(out["chunk_count"], 0);
}

#[test]
fn relevance_score_is_bounded() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let ep = make_episode(id, "x", "x");
    store.subscribe(podcast, vec![ep]);

    let results = collect_knowledge_matches(&store, "x");
    assert_eq!(results.len(), 1);
    assert!(results[0].relevance_score >= 0.0);
    assert!(results[0].relevance_score <= 1.0);
}

#[test]
fn collect_chunk_texts_returns_full_text_capped_at_limit() {
    let mut ks = KnowledgeStore::new();
    // Six chunks all matching "halving" — the helper must cap at the limit.
    for i in 0..6u32 {
        ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: "ep-1".to_owned(),
            chunk_index: i,
            start_secs: 0.0,
            end_secs: 0.0,
            text: format!("chunk {i} discusses the bitcoin halving in detail"),
            word_count: 8,
        }));
    }
    // Non-matching chunk must be excluded.
    ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
        episode_id: "ep-1".to_owned(),
        chunk_index: 99,
        start_secs: 0.0,
        end_secs: 0.0,
        text: "unrelated lightning network routing".to_owned(),
        word_count: 4,
    }));

    let scope = vec!["ep-1".to_owned()];
    let hits = collect_chunk_texts_for_topic(&ks, "halving", &scope, 5);
    assert_eq!(hits.len(), 5, "must cap at the requested limit");
    // Every hit is attributed to the owning episode.
    assert!(hits.iter().all(|(ep, _)| ep == "ep-1"));
    // Returns the full chunk text, not a 200-char snippet.
    assert!(hits
        .iter()
        .all(|(_, text)| text.contains("bitcoin halving in detail")));
    assert!(
        hits.iter().all(|(_, text)| !text.contains("lightning")),
        "non-matching chunks excluded"
    );
}

#[test]
fn collect_chunk_texts_scopes_to_supplied_episode_ids() {
    let mut ks = KnowledgeStore::new();
    // Same matching text under two different episodes.
    for ep in ["ep-mine", "ep-other"] {
        ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: ep.to_owned(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 0.0,
            text: "deep dive on the bitcoin halving".to_owned(),
            word_count: 6,
        }));
    }
    // Only ep-mine is in scope; the unrelated podcast's chunk must not leak.
    let scope = vec!["ep-mine".to_owned()];
    let hits = collect_chunk_texts_for_topic(&ks, "halving", &scope, 5);
    assert_eq!(
        hits.len(),
        1,
        "chunk search must stay scoped to the podcast"
    );
    assert_eq!(
        hits[0].0, "ep-mine",
        "hit attributed to the in-scope episode"
    );

    // Empty scope yields nothing even when chunks match.
    assert!(collect_chunk_texts_for_topic(&ks, "halving", &[], 5).is_empty());
}

#[test]
fn collect_chunk_texts_empty_query_returns_nothing() {
    let mut ks = KnowledgeStore::new();
    ks.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
        episode_id: "ep-1".to_owned(),
        chunk_index: 0,
        start_secs: 0.0,
        end_secs: 0.0,
        text: "anything".to_owned(),
        word_count: 1,
    }));
    assert!(collect_chunk_texts_for_topic(&ks, "  ", &["ep-1".to_owned()], 5).is_empty());
}

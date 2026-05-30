//! Tests for [`super::knowledge`] — knowledge-search matching, ranking, and snapshot projection.
//!
//! Extracted from `knowledge.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
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

#[test]
fn empty_query_returns_no_results() {
    let store = PodcastStore::new();
    assert!(collect_knowledge_matches(&store, "").is_empty());
    assert!(collect_knowledge_matches(&store, "   ").is_empty());
}

#[test]
fn substring_match_is_case_insensitive() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Tech Talk");
    let id = podcast.id;
    let ep = make_episode(id, "Episode 1", "We discuss MACHINE learning techniques.");
    store.subscribe(podcast, vec![ep.clone()]);

    let results = collect_knowledge_matches(&store, "machine");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].episode_id, ep.id.0.to_string());
    assert_eq!(results[0].podcast_title, "Tech Talk");
    assert!(results[0].relevance_score > 0.0);
}

#[test]
fn returns_at_most_top_k_results() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let episodes: Vec<Episode> = (0..15)
        .map(|i| make_episode(id, &format!("nostr episode {i}"), "about nostr"))
        .collect();
    store.subscribe(podcast, episodes);

    let results = collect_knowledge_matches(&store, "nostr");
    assert_eq!(results.len(), KNOWLEDGE_SEARCH_TOP_K);
}

#[test]
fn title_match_outranks_description_match() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    // Episode A: needle in description only.
    let ep_a = make_episode(id, "Random title", "deep dive on nostr relays");
    // Episode B: needle in title.
    let ep_b = make_episode(id, "nostr fundamentals", "intro chat");
    store.subscribe(podcast, vec![ep_a.clone(), ep_b.clone()]);

    let results = collect_knowledge_matches(&store, "nostr");
    assert_eq!(results.len(), 2);
    // Title-match (ep_b) must outrank description-match (ep_a).
    assert_eq!(results[0].episode_id, ep_b.id.0.to_string());
}

#[test]
fn no_match_returns_empty() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let ep = make_episode(id, "About cats", "feline behavior research");
    store.subscribe(podcast, vec![ep]);

    let results = collect_knowledge_matches(&store, "quantum");
    assert!(results.is_empty());
}

#[test]
fn snippet_truncates_long_text_with_ellipsis() {
    let long = "a".repeat(500);
    let body = format!("{}MATCH{}", long, long);
    let snippet = build_snippet(&body, long.len(), "MATCH".len());
    assert!(snippet.chars().count() <= KNOWLEDGE_SNIPPET_MAX_CHARS + 2);
    assert!(snippet.contains("MATCH"));
    assert!(snippet.starts_with('…'));
    assert!(snippet.ends_with('…'));
}

#[test]
fn snippet_passes_through_short_text_unchanged() {
    let body = "Short description with a match here.";
    let pos = body.find("match").unwrap();
    let snippet = build_snippet(body, pos, "match".len());
    assert_eq!(snippet, body);
}

#[test]
fn snippet_safe_on_multibyte_utf8() {
    // Em-dashes and other multi-byte chars must not panic the slicer.
    let prefix: String = std::iter::repeat("ä").take(300).collect();
    let body = format!("{prefix}NEEDLE{prefix}");
    let pos = body.find("NEEDLE").unwrap();
    let snippet = build_snippet(&body, pos, "NEEDLE".len());
    assert!(snippet.contains("NEEDLE"));
}

#[test]
fn snapshot_round_trips_knowledge_search_results() {
    use crate::ffi::PodcastUpdate;
    let row = KnowledgeSearchResult {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        snippet: "the exact text fragment".into(),
        start_secs: Some(42.0),
        relevance_score: 0.93,
    };
    let snap = PodcastUpdate {
        knowledge_search_results: vec![row.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("knowledge_search_results"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.knowledge_search_results, vec![row]);
}

#[test]
fn snapshot_omits_empty_knowledge_search_results() {
    // D5 byte-identity: an empty knowledge_search_results array must
    // not bloat the wire payload (preserves the legacy stub shape).
    use crate::ffi::PodcastUpdate;
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("knowledge_search_results"));
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
    store.lock().unwrap().set_transcript("ep-1".to_owned(), text);
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
    store.lock().unwrap().set_transcript("ep-2".to_owned(), long_text);
    let ks = empty_knowledge();
    let rev = Arc::new(AtomicU64::new(1));
    handle_index_episode("ep-2".to_owned(), &store, &ks, &rev);
    let first_count = ks.lock().unwrap().len();
    assert!(first_count >= 2, "expected ≥2 chunks from long transcript");

    // Now replace with a short transcript that fits in one chunk.
    store.lock().unwrap().set_transcript("ep-2".to_owned(), "short".to_owned());
    handle_index_episode("ep-2".to_owned(), &store, &ks, &rev);
    let second_count = ks.lock().unwrap().len();
    // Stale trailing chunks must be gone — only the new single chunk remains.
    assert_eq!(second_count, 1, "reindex must clear stale trailing chunks (got {second_count}, had {first_count})");
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

    store
        .lock()
        .unwrap()
        .set_transcript(ep_id.clone(), "we explore distributed consensus protocols".to_owned());

    let ks = empty_knowledge();
    let slot = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(1));

    handle_index_episode(ep_id.clone(), &store, &ks, &rev);
    let out = handle_search("consensus".to_owned(), &store, &slot, &ks, &rev);
    assert_eq!(out["ok"], true);

    let results = slot.lock().unwrap();
    assert_eq!(results.len(), 1, "transcript-only term must be found via chunk search");
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
    let texts = collect_chunk_texts_for_topic(&ks, "halving", &scope, 5);
    assert_eq!(texts.len(), 5, "must cap at the requested limit");
    // Returns the full chunk text, not a 200-char snippet.
    assert!(texts.iter().all(|t| t.contains("bitcoin halving in detail")));
    assert!(
        texts.iter().all(|t| !t.contains("lightning")),
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
    let texts = collect_chunk_texts_for_topic(&ks, "halving", &scope, 5);
    assert_eq!(texts.len(), 1, "chunk search must stay scoped to the podcast");

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

use super::*;

use crate::types::TranscriptChunk;

fn make_chunk(episode_id: &str, idx: u32) -> KnowledgeChunk {
    KnowledgeChunk::without_embedding(TranscriptChunk {
        episode_id: episode_id.into(),
        chunk_index: idx,
        start_secs: idx as f64,
        end_secs: (idx + 1) as f64,
        text: format!("chunk-{idx}"),
        word_count: 5,
    })
}

#[test]
fn upsert_replaces_existing_chunk() {
    let mut store = KnowledgeStore::new();
    store.upsert(make_chunk("ep-1", 0));
    store.upsert(make_chunk("ep-1", 0));
    assert_eq!(store.len(), 1);
}

#[test]
fn delete_episode_returns_count_removed() {
    let mut store = KnowledgeStore::new();
    store.upsert_many([
        make_chunk("ep-1", 0),
        make_chunk("ep-1", 1),
        make_chunk("ep-2", 0),
    ]);
    let removed = store.delete_episode("ep-1");
    assert_eq!(removed, 2);
    assert_eq!(store.len(), 1);
}

#[test]
fn embedded_skips_chunks_without_vectors() {
    let mut store = KnowledgeStore::new();
    store.upsert(make_chunk("ep-1", 0));
    store.upsert(KnowledgeChunk::with_embedding(
        make_chunk("ep-1", 1).chunk,
        vec![1.0, 0.0],
    ));
    assert_eq!(store.embedded().count(), 1);
}

//! Tests for the SQLite durable sidecar (`podcast_knowledge::sqlite`).

use std::io::Write;

use podcast_transcripts::TranscriptChunk;

use crate::sqlite::KnowledgeSqliteStore;
use crate::types::{EmbeddingVector, KnowledgeChunk};

fn make_chunk(episode_id: &str, idx: u32, text: &str) -> KnowledgeChunk {
    KnowledgeChunk::without_embedding(TranscriptChunk {
        episode_id: episode_id.to_owned(),
        chunk_index: idx,
        start_secs: idx as f64 * 10.0,
        end_secs: idx as f64 * 10.0 + 9.9,
        word_count: text.split_whitespace().count() as u32,
        text: text.to_owned(),
    })
}

fn make_chunk_with_embedding(episode_id: &str, idx: u32, embedding: Vec<f32>) -> KnowledgeChunk {
    KnowledgeChunk::with_embedding(
        TranscriptChunk {
            episode_id: episode_id.to_owned(),
            chunk_index: idx,
            start_secs: 0.0,
            end_secs: 9.9,
            word_count: 5,
            text: "chunk with embedding".to_owned(),
        },
        EmbeddingVector::new(embedding),
    )
}

/// Round-trip: insert chunks, drop the store, reopen on same path, load_all
/// must return the same chunks in the same order.
#[test]
fn durability_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("knowledge.sqlite");

    let chunk_a = make_chunk("ep-1", 0, "hello world machine learning");
    let chunk_b = make_chunk("ep-1", 1, "deep neural networks podcast");
    let chunk_c = make_chunk("ep-2", 0, "another episode chunk");

    // Session 1: write chunks.
    {
        let store = KnowledgeSqliteStore::open(&db_path);
        store.upsert(&chunk_a).unwrap();
        store.upsert(&chunk_b).unwrap();
        store.upsert(&chunk_c).unwrap();
    }
    // `store` dropped — connection closed.

    // Session 2: reopen and load_all.
    {
        let store2 = KnowledgeSqliteStore::open(&db_path);
        let mut loaded = store2.load_all();
        // Sort for deterministic comparison (SQLite has no guaranteed order).
        loaded.sort_by(|a, b| {
            a.chunk.episode_id.cmp(&b.chunk.episode_id)
                .then(a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
        });

        assert_eq!(loaded.len(), 3, "must reload all 3 chunks");

        assert_eq!(loaded[0].chunk.episode_id, "ep-1");
        assert_eq!(loaded[0].chunk.chunk_index, 0);
        assert_eq!(loaded[0].chunk.text, "hello world machine learning");

        assert_eq!(loaded[1].chunk.episode_id, "ep-1");
        assert_eq!(loaded[1].chunk.chunk_index, 1);
        assert_eq!(loaded[1].chunk.text, "deep neural networks podcast");

        assert_eq!(loaded[2].chunk.episode_id, "ep-2");
        assert_eq!(loaded[2].chunk.chunk_index, 0);
        assert_eq!(loaded[2].chunk.text, "another episode chunk");
    }
}

/// Embedding round-trip: a chunk with an `EmbeddingVector` serialises to BLOB
/// and deserialises back to the same f32 values.
#[test]
fn embedding_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("knowledge_emb.sqlite");

    let emb = vec![0.1_f32, 0.2, 0.3, -0.4, 0.5];
    let chunk = make_chunk_with_embedding("ep-emb", 0, emb.clone());

    {
        let store = KnowledgeSqliteStore::open(&db_path);
        store.upsert(&chunk).unwrap();
    }

    {
        let store2 = KnowledgeSqliteStore::open(&db_path);
        let loaded = store2.load_all();
        assert_eq!(loaded.len(), 1);
        let ev = loaded[0].embedding.as_ref().expect("embedding must be present");
        assert_eq!(ev.dim(), emb.len());
        for (got, expected) in ev.as_slice().iter().zip(&emb) {
            assert!(
                (got - expected).abs() < 1e-6,
                "f32 round-trip mismatch: got {got}, expected {expected}"
            );
        }
    }
}

/// Upsert idempotency: upserting the same `(episode_id, chunk_index)` a second
/// time replaces the row rather than duplicating it.
#[test]
fn upsert_replaces_existing_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("knowledge_upsert.sqlite");

    let store = KnowledgeSqliteStore::open(&db_path);
    store.upsert(&make_chunk("ep-u", 0, "original text")).unwrap();
    store.upsert(&make_chunk("ep-u", 0, "updated text")).unwrap();

    let loaded = store.load_all();
    assert_eq!(loaded.len(), 1, "upsert must not duplicate rows");
    assert_eq!(loaded[0].chunk.text, "updated text");
}

/// delete_episode removes all rows for the given episode and only those rows.
#[test]
fn delete_episode_removes_only_that_episode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("knowledge_del.sqlite");

    let store = KnowledgeSqliteStore::open(&db_path);
    store.upsert(&make_chunk("ep-del", 0, "chunk zero")).unwrap();
    store.upsert(&make_chunk("ep-del", 1, "chunk one")).unwrap();
    store.upsert(&make_chunk("ep-keep", 0, "keep me")).unwrap();

    store.delete_episode("ep-del").unwrap();

    let loaded = store.load_all();
    assert_eq!(loaded.len(), 1, "only ep-keep must remain");
    assert_eq!(loaded[0].chunk.episode_id, "ep-keep");
}

/// Corrupt-file quarantine: writing garbage bytes to the DB path must not
/// panic.  The resulting store is an empty in-memory fallback; the corrupt
/// file is renamed to `<path>.corrupt-<ts>`.
#[test]
fn corrupt_file_quarantined() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("knowledge_corrupt.sqlite");

    // Write garbage so Connection::open succeeds but migration fails (or
    // the file is not a valid SQLite header).
    {
        let mut f = std::fs::File::create(&db_path).expect("create file");
        f.write_all(b"this is definitely not a sqlite database!!!!!").unwrap();
    }
    assert!(db_path.exists(), "garbage file must exist before open");

    // Must not panic.
    let store = KnowledgeSqliteStore::open(&db_path);

    // In-memory fallback: empty store.
    let chunks = store.load_all();
    assert!(chunks.is_empty(), "corrupt-file fallback must be empty");

    // Corrupt file must have been quarantined (renamed away).
    assert!(
        !db_path.exists(),
        "original corrupt file must be renamed/quarantined"
    );

    // A `.corrupt-<ts>` sibling must exist.
    let siblings: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let quarantine_exists = siblings
        .iter()
        .any(|name| name.starts_with("knowledge_corrupt.sqlite.corrupt-"));
    assert!(quarantine_exists, "quarantine file must exist; found: {siblings:?}");
}

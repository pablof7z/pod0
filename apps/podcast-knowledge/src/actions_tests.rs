use super::*;

use podcast_transcripts::TranscriptChunk;

fn sample_chunk() -> TranscriptChunk {
    TranscriptChunk {
        episode_id: "ep-1".into(),
        chunk_index: 0,
        start_secs: 0.0,
        end_secs: 1.0,
        text: "hello".into(),
        word_count: 1,
    }
}

#[test]
fn ingest_chunks_round_trip() {
    let action = IngestChunks {
        episode_id: "ep-1".into(),
        chunks: vec![KnowledgeChunk::with_embedding(sample_chunk(), vec![1.0, 2.0])],
        replace_episode: true,
    };
    let json = serde_json::to_string(&action).unwrap();
    let back: IngestChunks = serde_json::from_str(&json).unwrap();
    assert_eq!(action, back);
}

#[test]
fn search_knowledge_round_trip() {
    let action = SearchKnowledge {
        query_embedding: EmbeddingVector::new(vec![0.1, 0.2, 0.3]),
        k: 5,
        episode_ids: Some(vec!["ep-1".into(), "ep-2".into()]),
    };
    let json = serde_json::to_string(&action).unwrap();
    let back: SearchKnowledge = serde_json::from_str(&json).unwrap();
    assert_eq!(action, back);
}

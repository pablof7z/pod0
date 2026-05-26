use super::*;


fn sample_chunk() -> TranscriptChunk {
    TranscriptChunk {
        episode_id: "ep-1".into(),
        chunk_index: 0,
        start_secs: 0.0,
        end_secs: 5.0,
        text: "hello world".into(),
        word_count: 2,
    }
}

#[test]
fn embedding_vector_dim() {
    let e = EmbeddingVector::new(vec![1.0, 2.0, 3.0]);
    assert_eq!(e.dim(), 3);
    assert_eq!(e.as_slice(), &[1.0, 2.0, 3.0]);
}

#[test]
fn knowledge_chunk_round_trip_with_embedding() {
    let kc = KnowledgeChunk::with_embedding(sample_chunk(), vec![0.1, 0.2]);
    let json = serde_json::to_string(&kc).unwrap();
    let back: KnowledgeChunk = serde_json::from_str(&json).unwrap();
    assert_eq!(kc, back);
}

#[test]
fn knowledge_chunk_omits_embedding_when_none() {
    let kc = KnowledgeChunk::without_embedding(sample_chunk());
    let json = serde_json::to_string(&kc).unwrap();
    assert!(!json.contains("embedding"));
}

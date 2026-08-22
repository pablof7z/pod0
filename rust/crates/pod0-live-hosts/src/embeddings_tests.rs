use serde_json::json;

use super::*;
use crate::embedding_response::{parse_ollama, parse_openai, parse_vector};

#[test]
fn non_finite_and_oversized_vectors_are_rejected() {
    assert!(parse_vector(Some(&json!([1.0, 2.0])), None, 1).is_err());
    assert!(parse_vector(Some(&json!(["NaN"])), None, 1).is_err());
}

#[test]
fn ollama_request_sends_requested_dimensions() {
    let mut request = embedding_request();
    request.dimensions = Some(2);
    assert_eq!(ollama_body(&request).get("dimensions"), Some(&json!(2)));
}

#[test]
fn both_providers_require_exact_requested_dimensions() {
    let openai = json!({"data":[{"index":0,"embedding":[1.0,2.0]}]});
    assert!(
        parse_openai(
            &serde_json::to_vec(&openai).unwrap(),
            evidence(),
            1,
            Some(3),
            4,
            1,
        )
        .is_err()
    );

    let ollama = json!({"embeddings":[[1.0,2.0]]});
    assert!(
        parse_ollama(
            &serde_json::to_vec(&ollama).unwrap(),
            evidence(),
            1,
            Some(3),
            4,
            1,
        )
        .is_err()
    );
}

#[test]
fn vectors_must_have_consistent_dimensions_without_a_request() {
    let value = json!({
        "data": [
            {"index":0,"embedding":[1.0]},
            {"index":1,"embedding":[1.0,2.0]}
        ]
    });
    assert!(
        parse_openai(
            &serde_json::to_vec(&value).unwrap(),
            evidence(),
            2,
            None,
            4,
            2,
        )
        .is_err()
    );
}

fn embedding_request() -> EmbeddingRequest {
    EmbeddingRequest {
        model: "model".to_owned(),
        inputs: vec!["input".to_owned()],
        dimensions: None,
        timeout: std::time::Duration::from_secs(1),
        limits: HttpLimits {
            maximum_body_bytes: 1_024,
            maximum_metadata_bytes: 1_024,
        },
        maximum_dimensions: 4,
        maximum_vectors: 1,
    }
}

fn evidence() -> HttpEvidence {
    HttpEvidence {
        status: 200,
        final_url: "https://example.invalid/embeddings".to_owned(),
        redirects: Vec::new(),
        entity_tag: None,
        last_modified: None,
        content_type: None,
        content_length: None,
    }
}

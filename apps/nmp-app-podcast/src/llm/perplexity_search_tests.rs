use super::*;
use serde_json::json;

#[test]
fn invalid_query_has_stable_kind() {
    assert_eq!(PerplexitySearchError::InvalidQuery.kind(), "invalid_query");
}

#[test]
fn status_errors_map_to_stable_kinds() {
    assert_eq!(
        PerplexitySearchError::ProviderStatus(401, String::new()).kind(),
        "invalid_key"
    );
    assert_eq!(
        PerplexitySearchError::ProviderStatus(429, String::new()).kind(),
        "rate_limited"
    );
}

#[test]
fn decodes_answer_and_search_results() {
    let response = json!({
        "model": "sonar",
        "choices": [{"message": {"content": "answer"}}],
        "search_results": [{"title": "Source", "url": "https://example.com"}],
        "usage": {"total_tokens": 10}
    });
    let result = decode_search_response(response, "perplexity", "sonar", 42).unwrap();

    assert_eq!(result.answer, "answer");
    assert_eq!(result.sources[0].title, "Source");
    assert_eq!(result.latency_ms, 42);
}

#[test]
fn falls_back_to_citations_for_sources() {
    let response = json!({
        "choices": [{"message": {"content": "answer"}}],
        "citations": ["https://example.com/a"]
    });
    let result = decode_search_response(response, "openrouter", "perplexity/sonar", 1).unwrap();

    assert_eq!(result.sources[0].url, "https://example.com/a");
}

#[test]
fn direct_replay_body_matches_fixture() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/provider_cassettes");
    let body = direct_perplexity_body("Find current sources about podcast chapter UX patterns.");
    let response = provider_replay::lookup_json_in_dir(
        dir,
        "perplexity",
        "web_search",
        "POST",
        "https://api.perplexity.ai/v1/sonar",
        &body,
    )
    .unwrap();
    assert_eq!(response.cassette_id, "perplexity-search-success");
}

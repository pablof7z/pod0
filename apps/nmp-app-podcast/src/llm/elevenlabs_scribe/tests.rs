use super::*;

#[test]
fn normalizes_elevenlabs_prefixed_model() {
    assert_eq!(normalize_model("elevenlabs:scribe_v2"), "scribe_v2");
    assert_eq!(normalize_model(" "), DEFAULT_SCRIBE_MODEL);
}

#[tokio::test]
async fn remote_audio_uses_source_url_without_downloading() {
    let source = resolve_scribe_audio_source("https://example.test/show.mp3")
        .await
        .unwrap();
    match source {
        ScribeAudioSource::SourceUrl(url) => {
            assert_eq!(url, "https://example.test/show.mp3");
        }
        ScribeAudioSource::File { .. } => panic!("remote source must not become a file upload"),
    }
}

#[test]
fn decodes_scribe_response_and_duration() {
    let result = decode_scribe_response(
        r#"{"language_code":"en","text":"hello","words":[{"text":"hello","start":0.0,"end":0.4,"type":"word","speaker_id":"spk_0"}]}"#,
        "scribe_v1".to_owned(),
        42,
    )
    .unwrap();
    assert_eq!(result.language_code.as_deref(), Some("en"));
    assert_eq!(result.words.len(), 1);
    assert_eq!(result.duration, Some(0.4));
    assert_eq!(result.latency_ms, 42);
}

#[test]
fn provider_status_maps_to_stable_kinds() {
    assert_eq!(
        ElevenLabsScribeError::ProviderStatus(401, String::new()).kind(),
        "invalid_key"
    );
    assert_eq!(
        ElevenLabsScribeError::ProviderStatus(429, String::new()).kind(),
        "rate_limited"
    );
}

#[test]
fn maps_audio_content_types_by_extension() {
    assert_eq!(content_type_for_extension(Some("mp3")), "audio/mpeg");
    assert_eq!(content_type_for_extension(Some("M4A")), "audio/mp4");
    assert_eq!(
        content_type_for_extension(Some("unknown")),
        "application/octet-stream"
    );
}

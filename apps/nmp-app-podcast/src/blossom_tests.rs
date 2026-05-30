//! Tests for [`super`] (`blossom.rs`) — auth-event construction and
//! response parsing. Transport is not exercised (the production path runs
//! through the HTTP capability); these cover the pure logic.

use super::*;

/// kind:24242 auth event carries `t=upload`, the file hash as `x`, and a
/// future `expiration` tag.
#[test]
fn build_auth_event_has_correct_tags() {
    let hash = "a".repeat(64);
    let created_at = 1_700_000_000i64;
    let tags = auth_event_tags(&hash, created_at);

    // t=upload
    assert!(
        tags.iter().any(|t| t.as_slice() == ["t".to_string(), "upload".to_string()]),
        "missing t=upload tag: {tags:?}"
    );
    // x=<file hash>
    assert!(
        tags.iter().any(|t| t.first().map(String::as_str) == Some("x")
            && t.get(1).map(String::as_str) == Some(hash.as_str())),
        "missing x=<hash> tag: {tags:?}"
    );
    // expiration is in the future relative to created_at
    let exp = tags
        .iter()
        .find(|t| t.first().map(String::as_str) == Some("expiration"))
        .and_then(|t| t.get(1))
        .expect("missing expiration tag")
        .parse::<i64>()
        .expect("expiration not an integer");
    assert!(exp > created_at, "expiration {exp} not after created_at {created_at}");

    // And the signed event really is kind 24242 with those tags.
    let secret = [7u8; 32];
    let json = build_auth_event(&secret, &hash, created_at).expect("sign auth event");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["kind"].as_u64(), Some(KIND_BLOSSOM_AUTH as u64));
    assert!(parsed["sig"].as_str().is_some_and(|s| !s.is_empty()), "event not signed");
}

#[test]
fn parse_blossom_response_happy_path() {
    let body = r#"{
        "url": "https://blossom.example/abc.mp3",
        "sha256": "deadbeef",
        "size": 123456,
        "type": "audio/mpeg"
    }"#;
    let result = parse_blossom_response(body).expect("parse happy path");
    assert_eq!(result.url, "https://blossom.example/abc.mp3");
    assert_eq!(result.hash, "deadbeef");
    assert_eq!(result.size, 123_456);
    assert_eq!(result.mime_type, "audio/mpeg");
}

#[test]
fn parse_blossom_response_missing_url_errors() {
    let body = r#"{"sha256":"deadbeef","size":10,"type":"audio/mpeg"}"#;
    let err = parse_blossom_response(body).expect_err("missing url must error");
    assert!(err.contains("url"), "error should mention url: {err}");
}

#[test]
fn parse_blossom_response_empty_url_errors() {
    let body = r#"{"url":"","sha256":"x"}"#;
    assert!(parse_blossom_response(body).is_err(), "empty url must error");
}

#[test]
fn parse_blossom_response_optional_fields_default() {
    // Only url present — size/hash/type fall back.
    let body = r#"{"url":"https://blossom.example/x"}"#;
    let result = parse_blossom_response(body).expect("url-only is valid");
    assert_eq!(result.url, "https://blossom.example/x");
    assert_eq!(result.size, 0);
    assert_eq!(result.hash, "");
    assert_eq!(result.mime_type, "audio/mp4");
}

#[test]
fn parse_blossom_response_non_json_errors() {
    assert!(parse_blossom_response("<html>nope</html>").is_err());
}

#[test]
fn sha256_hex_is_lowercase_64_chars() {
    let h = sha256_hex(b"hello world");
    assert_eq!(h.len(), 64);
    assert_eq!(h, h.to_lowercase());
    // Known vector for "hello world".
    assert_eq!(
        h,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

/// Full upload flow with an injected fetch closure: a temp file is hashed,
/// the auth header is built, and the descriptor is parsed back.
#[test]
fn upload_to_blossom_happy_path_with_injected_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.mp3");
    std::fs::write(&path, b"fake audio bytes").unwrap();
    let secret = [3u8; 32];

    let mut seen_auth = String::new();
    let result = upload_to_blossom(
        path.to_str().unwrap(),
        "https://blossom.example/",
        &secret,
        |req| {
            // POST to {server}/upload, no double slash.
            assert_eq!(req.method, HttpMethod::Post);
            assert_eq!(req.url, "https://blossom.example/upload");
            // Authorization: Nostr <base64> present.
            let auth = req
                .headers
                .iter()
                .find(|h| h.first().map(String::as_str) == Some("Authorization"))
                .and_then(|h| h.get(1))
                .cloned()
                .expect("missing Authorization header");
            assert!(auth.starts_with("Nostr "), "auth header: {auth}");
            seen_auth = auth;
            // Body is non-empty (base64 of the bytes).
            assert!(req.body.as_deref().is_some_and(|b| !b.is_empty()));
            Ok(HttpResult::Ok {
                status_code: 200,
                headers: vec![],
                body: r#"{"url":"https://blossom.example/blob.mp3","sha256":"ab","size":16,"type":"audio/mpeg"}"#.into(),
            })
        },
    )
    .expect("upload should succeed");

    assert_eq!(result.url, "https://blossom.example/blob.mp3");
    assert_eq!(result.size, 16);
    // The base64 auth payload decodes to a kind:24242 event.
    let b64 = seen_auth.strip_prefix("Nostr ").unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let event: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(event["kind"].as_u64(), Some(KIND_BLOSSOM_AUTH as u64));
}

#[test]
fn upload_to_blossom_http_error_propagates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.mp3");
    std::fs::write(&path, b"bytes").unwrap();
    let secret = [1u8; 32];

    let err = upload_to_blossom(
        path.to_str().unwrap(),
        "https://blossom.example",
        &secret,
        |_req| Ok(HttpResult::Ok { status_code: 500, headers: vec![], body: "boom".into() }),
    )
    .expect_err("500 must error");
    assert!(err.contains("500"), "err: {err}");
}

#[test]
fn upload_to_blossom_missing_file_errors() {
    let secret = [1u8; 32];
    let err = upload_to_blossom(
        "/nonexistent/path/ep.mp3",
        "https://blossom.example",
        &secret,
        |_req| panic!("fetch must not be called when file read fails"),
    )
    .expect_err("missing file must error");
    assert!(err.contains("read local file"), "err: {err}");
}

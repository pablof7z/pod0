//! Tests for [`super`] (`blossom.rs`) — response parsing.
//!
//! The hand-rolled BUD-02 upload path (auth-event construction + HTTP
//! transport) has been deleted; those concerns now live in the NMP kernel's
//! `nmp.blossom.upload` action. Only response-parsing tests remain here.

use super::*;

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
    assert!(
        parse_blossom_response(body).is_err(),
        "empty url must error"
    );
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

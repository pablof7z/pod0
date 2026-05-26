//! Tests for [`super::http`] — HttpMethod, HttpRequest, and HttpResult serde coverage.
//!
//! Extracted from `http.rs` to keep that file under the 500-line hard limit.

use super::*;

// ---- HttpMethod ---------------------------------------------------------------

#[test]
fn http_method_uppercase_serde_get() {
    let json = serde_json::to_string(&HttpMethod::Get).expect("encode");
    assert_eq!(json, r#""GET""#);
    let back: HttpMethod = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, HttpMethod::Get);
}

#[test]
fn http_method_uppercase_serde_post() {
    let json = serde_json::to_string(&HttpMethod::Post).expect("encode");
    assert_eq!(json, r#""POST""#);
    let back: HttpMethod = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, HttpMethod::Post);
}

#[test]
fn http_method_as_str_matches_wire() {
    assert_eq!(HttpMethod::Get.as_str(), "GET");
    assert_eq!(HttpMethod::Post.as_str(), "POST");
}

// ---- HttpRequest --------------------------------------------------------------

#[test]
fn http_request_get_no_headers_omits_fields() {
    let req = HttpRequest::get("https://example.com/feed.xml", std::iter::empty::<(&str, &str)>());
    let json = serde_json::to_string(&req).expect("encode");
    // `skip_serializing_if` keeps headers + body off the wire when absent.
    assert_eq!(json, r#"{"method":"GET","url":"https://example.com/feed.xml"}"#);
    let back: HttpRequest = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, req);
}

#[test]
fn http_request_get_serializes_headers_as_pair_arrays() {
    let req = HttpRequest::get(
        "https://example.com/feed.xml",
        [("Accept", "application/rss+xml"), ("If-None-Match", "\"abc123\"")],
    );
    let json = serde_json::to_string(&req).expect("encode");
    // Match the literal Swift encoder produces from the same shape.
    assert_eq!(
        json,
        r#"{"method":"GET","url":"https://example.com/feed.xml","headers":[["Accept","application/rss+xml"],["If-None-Match","\"abc123\""]]}"#
    );
    let back: HttpRequest = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, req);
}

#[test]
fn http_request_post_round_trips_body() {
    let req = HttpRequest {
        method: HttpMethod::Post,
        url: "https://example.com/api".into(),
        headers: vec![vec!["Content-Type".into(), "application/json".into()]],
        body: Some(r#"{"x":1}"#.into()),
    };
    let json = serde_json::to_string(&req).expect("encode");
    let back: HttpRequest = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, req);
}

#[test]
fn http_request_absent_headers_decode_to_empty() {
    // Wire from Swift when no headers were supplied: field omitted.
    let json = r#"{"method":"GET","url":"https://example.com/feed.xml"}"#;
    let req: HttpRequest = serde_json::from_str(json).expect("decode");
    assert!(req.headers.is_empty());
    assert!(req.body.is_none());
}

// ---- HttpResult ---------------------------------------------------------------

#[test]
fn http_result_ok_matches_swift_wire_shape() {
    let result = HttpResult::Ok {
        status_code: 200,
        headers: vec![
            vec!["ETag".into(), "\"abc123\"".into()],
            vec!["Last-Modified".into(), "Wed, 31 Dec 2025 23:00:00 GMT".into()],
        ],
        body: "<rss/>".into(),
    };
    let json = serde_json::to_string(&result).expect("encode");
    assert_eq!(
        json,
        r#"{"status":"ok","status_code":200,"headers":[["ETag","\"abc123\""],["Last-Modified","Wed, 31 Dec 2025 23:00:00 GMT"]],"body":"<rss/>"}"#
    );
    let back: HttpResult = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, result);
}

#[test]
fn http_result_ok_omits_empty_headers() {
    let result = HttpResult::Ok {
        status_code: 304,
        headers: vec![],
        body: String::new(),
    };
    let json = serde_json::to_string(&result).expect("encode");
    // Matches the legacy Swift `.ok(statusCode:body:)` wire — additive
    // headers field stays off the wire when absent.
    assert_eq!(json, r#"{"status":"ok","status_code":304,"body":""}"#);
}

#[test]
fn http_result_error_matches_swift_wire_shape() {
    let result = HttpResult::Error {
        message: "transport: timeout".into(),
    };
    let json = serde_json::to_string(&result).expect("encode");
    assert_eq!(json, r#"{"status":"error","message":"transport: timeout"}"#);
    let back: HttpResult = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, result);
}

#[test]
fn http_result_decodes_legacy_ok_without_headers_field() {
    // The shipped Swift `HttpResult.ok` (pre-M5) encodes only status_code
    // + body. The Rust decoder must keep accepting that shape so an
    // older iOS build (or a 304 with no headers) doesn't trip a deserialize
    // error.
    let json = r#"{"status":"ok","status_code":200,"body":"<rss/>"}"#;
    let result: HttpResult = serde_json::from_str(json).expect("decode");
    match result {
        HttpResult::Ok {
            status_code,
            headers,
            body,
        } => {
            assert_eq!(status_code, 200);
            assert!(headers.is_empty());
            assert_eq!(body, "<rss/>");
        }
        HttpResult::Error { .. } => panic!("expected Ok"),
    }
}

#[test]
fn http_result_header_lookup_is_case_insensitive() {
    let result = HttpResult::Ok {
        status_code: 200,
        headers: vec![
            vec!["ETag".into(), "\"abc\"".into()],
            vec!["Last-Modified".into(), "Wed, 31 Dec 2025 23:00:00 GMT".into()],
        ],
        body: String::new(),
    };
    assert_eq!(result.header("etag"), Some("\"abc\""));
    assert_eq!(result.header("ETAG"), Some("\"abc\""));
    assert_eq!(result.header("Last-Modified"), Some("Wed, 31 Dec 2025 23:00:00 GMT"));
    assert_eq!(result.header("missing"), None);
}

#[test]
fn http_result_header_lookup_on_error_returns_none() {
    let result = HttpResult::Error {
        message: "boom".into(),
    };
    assert_eq!(result.header("ETag"), None);
}

#[test]
fn namespace_matches_canonical_capability_plan() {
    assert_eq!(HTTP_CAPABILITY_NAMESPACE, "nmp.http.capability");
}

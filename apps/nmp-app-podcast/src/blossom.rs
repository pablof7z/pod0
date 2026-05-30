//! Blossom blob upload (BUD-01 / BUD-02) — M8.
//!
//! Before a `kind:54` NIP-F4 episode event is published it MUST point at a
//! real audio URL. This module uploads the episode's local download file to
//! the user-configured Blossom server and hands back the permanent URL +
//! SHA-256 hash so [`crate::host_op_publish::publish_episode`] can stamp the
//! `audio` tag with the hosted blob instead of the original RSS enclosure.
//!
//! ## Protocol (BUD-01 / BUD-02)
//!
//! 1. Compute the SHA-256 of the raw file bytes — this is the Blossom "blob
//!    hash" the server addresses the blob by.
//! 2. Build a `kind:24242` Nostr auth event signed with the per-podcast
//!    NIP-F4 secret. Required tags: `["t","upload"]`, `["x",<sha256-hex>]`,
//!    and `["expiration",<unix-ts>]` (many servers `401` without the
//!    expiration tag — see the `expiration` constant below).
//! 3. `PUT`/`POST {server}/upload` with header
//!    `Authorization: Nostr <base64(event_json)>` and the raw bytes as the
//!    body. This module uses `POST` to match the only verb the HTTP
//!    capability `HttpMethod` enum currently exposes.
//! 4. Parse the JSON descriptor `{"url","sha256","size","type"}`.
//!
//! ## Transport limitation (READ THIS)
//!
//! The HTTP capability ([`podcast_feeds::http::HttpRequest`]) carries the
//! request body as a UTF-8 `String`, and the shipped iOS executor encodes it
//! with `body.data(using: .utf8)` (`HttpCapability.swift:229`). Binary audio
//! bytes therefore **cannot transit this capability unchanged today**. We
//! base64-encode the body here so the wire payload is valid UTF-8 and the
//! Rust side is correct; the feature is end-to-end functional only once the
//! capability/Swift executor learns to base64-decode upload bodies. Tracked
//! in `docs/BACKLOG.md`. The unit tests below exercise auth-event
//! construction and response parsing (the parts that are transport-agnostic)
//! via an injected `fetch` closure.

use base64::Engine;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, SecretKey, Tag, Timestamp};
use podcast_feeds::http::{HttpMethod, HttpRequest, HttpResult};
use sha2::{Digest, Sha256};

/// `kind:24242` — Blossom authorization event (BUD-01).
const KIND_BLOSSOM_AUTH: u16 = 24242;

/// Auth-event lifetime in seconds. The `expiration` tag is set this far in
/// the future from `created_at`; Blossom servers reject already-expired auth.
const AUTH_EXPIRATION_SECS: i64 = 60 * 60;

/// Outcome of a successful Blossom upload — the blob descriptor the server
/// returns, normalized into the fields the publish path needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlossomUploadResult {
    /// Permanent URL the blob is now served from.
    pub url: String,
    /// SHA-256 hex of the blob (echoed by the server; equals the locally
    /// computed hash on a well-behaved server).
    pub hash: String,
    /// Blob size in bytes.
    pub size: u64,
    /// MIME type the server recorded for the blob.
    pub mime_type: String,
}

/// Upload a local file to a Blossom server and return its permanent URL.
///
/// `fetch` is the HTTP transport (in production a closure over
/// `handler.dispatch_http`). It is injected so the upload logic stays a pure,
/// unit-testable function with no `*mut NmpApp` dependency.
pub fn upload_to_blossom(
    local_path: &str,
    blossom_url: &str,
    secret_bytes: &[u8; 32],
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> Result<BlossomUploadResult, String> {
    let bytes = std::fs::read(local_path)
        .map_err(|e| format!("read local file {local_path}: {e}"))?;
    if bytes.is_empty() {
        return Err(format!("local file is empty: {local_path}"));
    }

    let hash_hex = sha256_hex(&bytes);
    let now = chrono::Utc::now().timestamp();
    let auth_event_json = build_auth_event(secret_bytes, &hash_hex, now)?;
    let auth_header = format!(
        "Nostr {}",
        base64::engine::general_purpose::STANDARD.encode(auth_event_json.as_bytes())
    );

    let upload_url = format!("{}/upload", blossom_url.trim_end_matches('/'));
    let req = HttpRequest {
        method: HttpMethod::Post,
        url: upload_url,
        headers: vec![
            vec!["Authorization".into(), auth_header],
            vec!["Content-Type".into(), "application/octet-stream".into()],
        ],
        // See the transport-limitation note in the module docs: the HTTP
        // capability body is a UTF-8 String, so the raw blob is base64-encoded
        // to survive the bridge. The executor must base64-decode it.
        body: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
    };

    let result = fetch(&req)?;
    let body = match result {
        HttpResult::Ok { status_code, body, .. } => {
            if !(200..300).contains(&status_code) {
                return Err(format!("blossom upload http {status_code}: {body}"));
            }
            body
        }
        HttpResult::Error { message } => {
            return Err(format!("blossom upload transport: {message}"))
        }
    };

    parse_blossom_response(&body)
}

/// Compute the lowercase SHA-256 hex digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Build and sign the `kind:24242` Blossom authorization event (BUD-01).
/// Returns the canonical signed event JSON.
fn build_auth_event(
    secret_bytes: &[u8; 32],
    file_hash_hex: &str,
    created_at_secs: i64,
) -> Result<String, String> {
    let tags = auth_event_tags(file_hash_hex, created_at_secs);
    let sk = SecretKey::from_slice(secret_bytes)
        .map_err(|e| format!("invalid secret key: {e}"))?;
    let keys = Keys::new(sk);

    let nostr_tags: Vec<Tag> = tags
        .iter()
        .filter_map(|t| Tag::parse(t).ok())
        .collect();

    let event = EventBuilder::new(Kind::from(KIND_BLOSSOM_AUTH), "Upload audio")
        .tags(nostr_tags)
        .custom_created_at(Timestamp::from(created_at_secs as u64))
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign blossom auth: {e}"))?;

    Ok(event.as_json())
}

/// The tag set for a Blossom upload auth event. Pure so the test can assert
/// the exact shape without signing.
fn auth_event_tags(file_hash_hex: &str, created_at_secs: i64) -> Vec<Vec<String>> {
    vec![
        vec!["t".into(), "upload".into()],
        vec!["x".into(), file_hash_hex.to_string()],
        vec![
            "expiration".into(),
            (created_at_secs + AUTH_EXPIRATION_SECS).to_string(),
        ],
    ]
}

/// Parse a Blossom blob descriptor `{"url","sha256","size","type"}`. The
/// `url` field is mandatory; `sha256` / `size` / `type` fall back to sensible
/// defaults when a server omits them.
fn parse_blossom_response(body: &str) -> Result<BlossomUploadResult, String> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("blossom response not JSON: {e}"))?;

    let url = json
        .get("url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "blossom response missing url".to_string())?
        .to_string();

    let hash = json
        .get("sha256")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let size = json.get("size").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let mime_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/mp4")
        .to_string();

    Ok(BlossomUploadResult {
        url,
        hash,
        size,
        mime_type,
    })
}

#[cfg(test)]
#[path = "blossom_tests.rs"]
mod tests;

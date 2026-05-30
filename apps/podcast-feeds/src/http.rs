//! HTTP capability schema — `nmp.http.capability`.
//!
//! Rust mirror of the wire vocabulary the iOS executor in
//! `ios/Podcast/Podcast/Capabilities/HttpCapability.swift` already implements.
//! `nmp-app-podcast::capability::http` re-exports these so consumers in either
//! crate dispatch by the same types; this module is the canonical home because
//! `podcast-feeds` is the first consumer (RSS / OPML refresh) and lives lower
//! in the crate graph than `nmp-app-podcast`.
//!
//! ## Doctrine
//!
//! * **D6 — errors as data.** [`HttpResult`] is a tagged enum; transport
//!   failure (DNS, TLS, timeout, malformed request) is the `Error` variant,
//!   not a panic. The Rust side never `?`'s on the wire — callers pattern
//!   match.
//! * **D7 — capabilities execute, never decide.** The iOS executor performs
//!   the exact GET/POST [`HttpRequest`] describes and reports the raw
//!   [`HttpResult`]. *Which* URL to call (RSS feed, iTunes search,
//!   transcript fetch) and *what to do* with the body are kernel
//!   decisions.
//! * **D15 — bounded input.** Header pairs that aren't exactly
//!   `[name, value]` are ignored on the iOS side; the Rust types here
//!   should never produce malformed pairs.
//!
//! ## Wire format
//!
//! The Swift counterpart in `HttpCapability.swift` is the source of truth
//! for byte-for-byte JSON. Examples:
//!
//! ```text
//! request:  {"method":"GET","url":"https://…","headers":[["Accept","application/rss+xml"]]}
//! result:   {"status":"ok","status_code":200,"headers":[["ETag","\"abc\""]],"body":"<rss>…</rss>"}
//! error:    {"status":"error","message":"transport: timeout"}
//! ```
//!
//! Notes for anyone touching this file:
//!
//! - The result is tagged on `"status"`, not `"type"`. Don't reflexively copy
//!   the `audio` / `download` capability shape — it's a different tag key.
//! - `HttpMethod` is `UPPERCASE` (`"GET"` / `"POST"`), not `snake_case`.
//! - Headers travel as arrays of two-element string arrays
//!   (`Vec<Vec<String>>`), not a map — RSS feeds sometimes send the same
//!   header twice and we want to preserve order/multiplicity. The iOS side
//!   reads `[[String]]` and writes them with `setValue(_:forHTTPHeaderField:)`.
//! - The response body is a UTF-8 string. The iOS executor lossy-converts the
//!   `Data` to a `String` via `String(data:encoding:.utf8)`. RSS feeds with
//!   non-UTF-8 encodings (Windows-1252, ISO-8859-1) lose the original bytes
//!   here. This is a pre-existing limitation inherited from the shipped
//!   Swift contract and the legacy Swift `RSSParser`; tracked for follow-up
//!   in BACKLOG. Don't widen the contract in this PR.
//!
//! ## Schema stability
//!
//! M5 introduces this Rust schema for the first time — the canonical
//! `nmp-core::capability::http` referenced from Chirp's `HttpCapability.swift`
//! has not landed upstream yet (no `nmp-core/src/substrate/http.rs` exists).
//! When that upstream type does land, this module reconciles against it.

use serde::{Deserialize, Serialize};

/// Capability namespace string. Mirrors `HttpCapability::namespace` in the
/// Swift executor so the iOS-side router (`PodcastCapabilities.handleJSON`)
/// dispatches the same way `KeyringCapability` and `AudioCapability` do.
pub const HTTP_CAPABILITY_NAMESPACE: &str = "nmp.http.capability";

// ---------------------------------------------------------------------------
// HTTP method
// ---------------------------------------------------------------------------

/// HTTP verb. Wire form is `UPPERCASE`: `"GET"` / `"POST"`.
///
/// Intentionally narrow — every M5 caller (RSS refresh, iTunes search) is
/// `GET`-only; `Post` is here for the iTunes search / Blossom upload (M10)
/// callers and matches the Swift `HttpMethod` enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    /// Returns the wire string (`"GET"` / `"POST"`) without going through
    /// serde — useful for inspecting a request without re-encoding.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

// ---------------------------------------------------------------------------
// Rust → iOS: HttpRequest
// ---------------------------------------------------------------------------

/// Capability-private request payload — what travels in
/// `CapabilityRequest.payload_json`.
///
/// Headers are array-of-pairs (not a map) because some HTTP clients send the
/// same header twice and we preserve order/multiplicity across the bridge.
/// Empty / absent headers decode as `Vec::new()` on both sides.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HttpRequest {
    /// HTTP verb. Wire form `UPPERCASE`.
    pub method: HttpMethod,
    /// Absolute URL (`http://` or `https://`). Validated as a URL on the
    /// iOS side; an unparseable string yields `HttpResult::Error{message:
    /// "invalid-url"}`.
    pub url: String,
    /// Header `[name, value]` pairs. The iOS side ignores pairs that
    /// aren't exactly length 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<Vec<String>>,
    /// UTF-8 request body. `None` (the wire-omitted form) for `GET`;
    /// `Some(body)` for a text `POST`. Mutually exclusive with
    /// [`Self::body_base64`]: a UTF-8 `String` cannot carry arbitrary binary
    /// bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Standard-alphabet base64 (`+/`, padded) request body. Present when the
    /// kernel needs to send *binary* bytes that don't survive a UTF-8
    /// round-trip — e.g. the Blossom blob upload (`crate`-external
    /// `blossom.rs`), which SHA-256s the raw audio file and base64-encodes the
    /// bytes so they transit this capability intact. The iOS executor decodes
    /// this back to raw `Data` before sending it as the HTTP body and, when
    /// both are present, prefers it over [`Self::body`]
    /// (`HttpCapability.swift`). Purely additive: callers sending only UTF-8
    /// bodies omit it and behave exactly as before. Wire field `body_base64`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
}

impl HttpRequest {
    /// Construct a `GET` request with the given headers.
    ///
    /// `headers` is `(name, value)` tuples; converted to the wire pair
    /// form at call sites that find a tuple shape cleaner.
    #[must_use]
    pub fn get<I, K, V>(url: impl Into<String>, headers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: headers
                .into_iter()
                .map(|(k, v)| vec![k.into(), v.into()])
                .collect(),
            body: None,
            body_base64: None,
        }
    }
}

// ---------------------------------------------------------------------------
// iOS → Rust: HttpResult
// ---------------------------------------------------------------------------

/// Capability-private result payload — what travels in
/// `CapabilityEnvelope.result_json`.
///
/// Tagged on `"status"` so the wire shape matches the existing Swift
/// `HttpResult` enum byte-for-byte:
///
/// ```text
/// {"status":"ok","status_code":200,"headers":[["ETag","\"abc\""]],"body":"…"}
/// {"status":"error","message":"…"}
/// ```
///
/// **Headers on `Ok`:** populated from `HTTPURLResponse.allHeaderFields` on
/// the iOS side so the Rust caller (FeedClient, etc.) can read `ETag` /
/// `Last-Modified` without a second round-trip. Empty when the response had
/// no headers (e.g. `file://` short-circuit on iOS).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HttpResult {
    /// Transport succeeded. `status_code` is the raw HTTP status — a 200 and
    /// a 404 are both `Ok` here; interpretation is the caller's policy (D7).
    Ok {
        status_code: u16,
        /// Response headers as `[name, value]` pairs. The iOS side writes
        /// `name`s as the canonical-case strings from
        /// `HTTPURLResponse.allHeaderFields`; callers that look up specific
        /// headers should match case-insensitively (see
        /// [`Self::header`]).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<Vec<String>>,
        /// UTF-8 body. Non-UTF-8 bytes are lossy-converted by the iOS
        /// executor — see the file-level doc on this pre-existing
        /// limitation.
        body: String,
    },
    /// Transport-level failure (DNS, TLS, timeout, malformed request,
    /// capability stopped, etc.). The message is a human-readable
    /// diagnostic, not a stable enum.
    Error { message: String },
}

impl HttpResult {
    /// Case-insensitive header lookup. Returns the first matching value or
    /// `None`. Header names from the wire are whatever case the upstream
    /// server returned (preserved by `HTTPURLResponse.allHeaderFields`),
    /// so callers must not assume `"ETag"` vs `"etag"`.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        let headers = match self {
            Self::Ok { headers, .. } => headers,
            Self::Error { .. } => return None,
        };
        for pair in headers {
            if pair.len() == 2 && pair[0].eq_ignore_ascii_case(name) {
                return Some(pair[1].as_str());
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;

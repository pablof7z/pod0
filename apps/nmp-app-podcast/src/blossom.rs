//! Blossom blob response parsing — BUD-01 descriptor deserialization.
//!
//! The full Build → Sign → Transport pipeline for Blossom uploads is now
//! owned by the NMP kernel via `nmp.blossom.upload` (D13). App code dispatches
//! the action with `signer_pubkey: Some(podcast_pubkey_hex)` and reads the
//! settled `BlobDescriptor` from the `action_results` snapshot slot.
//!
//! This module retains only the response-parsing helper used by the Rust
//! integration tests and by any future path that needs to decode a blob
//! descriptor from raw JSON (e.g. a headless scenario asserting upload shape).
//!
//! The hand-rolled BUD-02 upload path (sha256 + kind:24242 auth-event signing +
//! base64 Authorization header + PUT) has been deleted. It lived in the old
//! `upload_to_blossom` function; the commit that removed it is the one that
//! introduced `nmp_dispatch::blossom_upload_via_nmp`.

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

/// Parse a Blossom blob descriptor `{"url","sha256","size","type"}`. The
/// `url` field is mandatory; `sha256` / `size` / `type` fall back to sensible
/// defaults when a server omits them.
pub fn parse_blossom_response(body: &str) -> Result<BlossomUploadResult, String> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("blossom response not JSON: {e}"))?;

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
    let size = json
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
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

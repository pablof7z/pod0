//! Raw NIP-F4 tag-shaped views.
//!
//! These structs hold the values extracted from a Nostr event *before* any
//! mapping into [`podcast_core::Podcast`] / [`podcast_core::Episode`].
//! Keeping a raw layer means parsing errors surface as `ParseError` at the
//! wire boundary, and the domain mapping step is a pure (infallible)
//! transform that can be inspected in isolation.
//!
//! No `nostr` crate dependency: we work directly off `Vec<Vec<String>>`
//! tags as delivered by the NMP kernel. The kernel owns the typed
//! `Event` reconstruction; this crate owns the NIP-F4 schema.

use serde::{Deserialize, Serialize};

/// Parsed `kind:10154` show event (NIP-F4).
///
/// Pubkey and `created_at` come from the wrapping Nostr event header (not
/// from tags) but are kept here so a parsed `NipF4DiscoveryShow` is self-contained
/// and can be re-mapped to a `Podcast` without re-threading the event.
///
/// NIP-F4 shows have no `d` tag; the show is identified by pubkey alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NipF4DiscoveryShow {
    /// Author pubkey (hex). Identifies the podcast key that signed the event.
    pub pubkey: String,
    /// `["title", ...]`.
    pub title: String,
    /// `["description", ...]` — falls back to `Event.content` during parse.
    pub description: String,
    /// `["image", <url>]`.
    pub image_url: Option<String>,
    /// `["language", <bcp47>]`.
    pub language: Option<String>,
    /// `["p", <pubkey>]` — set by the publisher to the podcast pubkey
    /// so other Nostr clients can pick the show up as a profile-tagged event.
    pub author_pubkey: Option<String>,
    /// Every `["t", <category>]` tag, in event order.
    pub categories: Vec<String>,
    /// Event header `created_at` (unix seconds).
    pub created_at: i64,
}

impl NipF4DiscoveryShow {
    /// NIP-F4 coordinate string `"10154:<pubkey>"`.
    /// Shows are identified by pubkey alone — no d-tag.
    pub fn coordinate(&self) -> String {
        format!("{}:{}", super::kinds::KIND_SHOW, self.pubkey)
    }
}

/// Parsed `kind:54` episode event (NIP-F4).
///
/// `PartialEq` only — `duration_secs` carries an `f64` (matching the
/// `podcast_core::Episode.duration_secs` shape) which doesn't implement
/// `Eq`. The few tests that compare `NipF4DiscoveryEpisode` values do so on
/// equality, not in `Eq`-bound contexts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NipF4DiscoveryEpisode {
    /// Value of the `["d", ...]` tag — stable per-author identifier.
    pub d_tag: String,
    /// `["title", ...]`.
    pub title: String,
    /// `["summary", ...]` — falls back to `Event.content` during parse.
    pub summary: String,
    /// `["published_at", <unix-seconds>]`. Defaults to `created_at` if
    /// the tag is missing or non-numeric.
    pub published_at: i64,
    /// `["duration", <seconds>]`.
    pub duration_secs: Option<f64>,
    /// `["image", <url>]`.
    pub image_url: Option<String>,
    /// Audio URL — preferred source is the `["imeta", "url <u>", ...]`
    /// tag (matches the Swift publisher); falls back to `["url", <u>]`
    /// for tolerance with foreign clients.
    pub audio_url: String,
    /// MIME type, if the `imeta` block carries an `m <mime>` field.
    pub audio_mime_type: Option<String>,
    /// SHA-256 hex of the audio bytes, if the `imeta` block carries an
    /// `x <hash>` field. Useful for download verification.
    pub audio_sha256_hex: Option<String>,
    /// File size in bytes, if the `imeta` block carries a `size <n>` field.
    pub audio_size_bytes: Option<u64>,
    /// `["a", "10154:<pubkey>:<show-d>"]` — reference back to the
    /// parent show event (NIP-F4 kind:10154).
    pub show_a_tag: Option<ShowReference>,
    /// `["chapters", <url>, <mime>]`.
    pub chapters_url: Option<String>,
    /// `["transcript", <url>, <mime>]`.
    pub transcript_url: Option<String>,
    /// MIME type from the second component of the `transcript` tag.
    pub transcript_mime_type: Option<String>,
}

/// Parsed `["a", "<kind>:<pubkey>:<d-tag>"]` reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowReference {
    pub kind: u32,
    pub pubkey: String,
    pub d_tag: String,
}

impl ShowReference {
    /// Render back to wire form `"<kind>:<pubkey>:<d-tag>"`.
    pub fn to_wire(&self) -> String {
        format!("{}:{}:{}", self.kind, self.pubkey, self.d_tag)
    }
}

/// Failures the parse + build sides surface.
///
/// Kept as an enum (not `anyhow`) so callers can match on the failure
/// shape — important once the kernel-side action modules need to choose
/// between dropping a bad event silently and reporting a publish error
/// back to the iOS layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Event kind didn't match the expected NIP-F4 kind.
    WrongKind { expected: u32, got: u32 },
    /// Required tag was missing from the event.
    MissingTag(&'static str),
    /// Required tag was present but its value was empty.
    EmptyTag(&'static str),
    /// `["a", ...]` value didn't match `"<kind>:<pubkey>:<d>"`.
    MalformedReference(String),
    /// `imeta` block didn't contain a `url <u>` field and no fallback
    /// `["url", ...]` tag was present either.
    MissingAudioUrl,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongKind { expected, got } => {
                write!(f, "wrong event kind: expected {expected}, got {got}")
            }
            Self::MissingTag(name) => write!(f, "missing required tag `{name}`"),
            Self::EmptyTag(name) => write!(f, "tag `{name}` is empty"),
            Self::MalformedReference(value) => {
                write!(f, "malformed `a` tag reference: `{value}`")
            }
            Self::MissingAudioUrl => write!(f, "episode is missing an audio url"),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;

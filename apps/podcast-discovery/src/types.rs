//! Raw NIP-74 tag-shaped views.
//!
//! These structs hold the values extracted from a Nostr event *before* any
//! mapping into [`podcast_core::Podcast`] / [`podcast_core::Episode`].
//! Keeping a raw layer means parsing errors surface as `ParseError` at the
//! wire boundary, and the domain mapping step is a pure (infallible)
//! transform that can be inspected in isolation.
//!
//! No `nostr` crate dependency: we work directly off `Vec<Vec<String>>`
//! tags as delivered by the NMP kernel. The kernel owns the typed
//! `Event` reconstruction; this crate owns the NIP-74 schema.

use serde::{Deserialize, Serialize};

/// Parsed `kind:30074` show event.
///
/// Pubkey and `created_at` come from the wrapping Nostr event header (not
/// from tags) but are kept here so a parsed `NIP74Show` is self-contained
/// and can be re-mapped to a `Podcast` without re-threading the event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NIP74Show {
    /// Author pubkey (hex). Identifies the agent that signed the event.
    pub pubkey: String,
    /// Value of the `["d", ...]` tag — stable per-author identifier.
    pub d_tag: String,
    /// `["title", ...]`.
    pub title: String,
    /// `["summary", ...]` — falls back to `Event.content` during parse.
    pub summary: String,
    /// `["image", <url>]`.
    pub image_url: Option<String>,
    /// `["language", <bcp47>]`.
    pub language: Option<String>,
    /// `["p", <pubkey>]` — set by the publisher to the author pubkey
    /// (mirrors the Swift code) so other Nostr clients can pick the show
    /// up as a profile-tagged event.
    pub author_pubkey: Option<String>,
    /// Every `["t", <category>]` tag, in event order.
    pub categories: Vec<String>,
    /// Event header `created_at` (unix seconds).
    pub created_at: i64,
}

impl NIP74Show {
    /// Stable NIP-33 coordinate string `"30074:<pubkey>:<d-tag>"`.
    /// Mirrors `NostrPodcastDiscoveryService.ShowResult.coordinate`.
    pub fn coordinate(&self) -> String {
        format!("{}:{}:{}", super::kinds::KIND_SHOW, self.pubkey, self.d_tag)
    }
}

/// Parsed `kind:30075` episode event.
///
/// `PartialEq` only — `duration_secs` carries an `f64` (matching the
/// `podcast_core::Episode.duration_secs` shape) which doesn't implement
/// `Eq`. The few tests that compare `NIP74Episode` values do so on
/// equality, not in `Eq`-bound contexts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NIP74Episode {
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
    /// `["a", "30074:<pubkey>:<show-d>"]` — reference back to the
    /// parent show event.
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
    /// Event kind didn't match the expected NIP-74 kind.
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
mod tests {
    use super::*;

    #[test]
    fn show_coordinate_matches_swift_format() {
        let show = NIP74Show {
            pubkey: "abc123".into(),
            d_tag: "podcast:guid:1234".into(),
            title: "X".into(),
            summary: String::new(),
            image_url: None,
            language: None,
            author_pubkey: None,
            categories: vec![],
            created_at: 0,
        };
        assert_eq!(show.coordinate(), "30074:abc123:podcast:guid:1234");
    }

    #[test]
    fn show_reference_round_trips_through_wire() {
        let r = ShowReference {
            kind: 30074,
            pubkey: "abc".into(),
            d_tag: "podcast:guid:1".into(),
        };
        assert_eq!(r.to_wire(), "30074:abc:podcast:guid:1");
    }

    #[test]
    fn parse_error_renders_human_message() {
        assert_eq!(
            ParseError::WrongKind {
                expected: 30074,
                got: 1,
            }
            .to_string(),
            "wrong event kind: expected 30074, got 1"
        );
        assert_eq!(
            ParseError::MissingTag("d").to_string(),
            "missing required tag `d`"
        );
    }
}

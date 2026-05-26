use serde::{Deserialize, Serialize};

/// One NIP-22 (kind 1111) comment surfaced via
/// [`super::snapshot::PodcastUpdate::comments`] for the
/// currently-playing episode.
///
/// The shape is intentionally narrow — id, author, body, timestamp.
/// Reply threading, reactions, and zaps live in follow-up projections.
///
/// `id` is the Nostr event id (lowercase hex). `author_npub` is the
/// bech32 encoding of the event's `pubkey` so the iOS shell can render
/// it without re-encoding. `author_name` is the cached display name
/// from NIP-01 metadata when the projection layer has one; `None`
/// means the UI should fall back to the truncated npub stub.
/// `created_at` is Unix seconds (matches NIP-01's `created_at`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct CommentSummary {
    /// Event id (lowercase hex) — stable Nostr identifier.
    pub id: String,
    /// Author bech32 (`npub1…`) — pre-encoded so iOS doesn't need a
    /// bech32 dependency to render the stub key.
    pub author_npub: String,
    /// Cached display name from the author's NIP-01 metadata, when
    /// known. `None` means the UI renders the truncated npub instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// Comment body — the raw `content` field of the kind 1111 event.
    pub content: String,
    /// Unix seconds (matches NIP-01 `created_at`).
    pub created_at: i64,
}

/// One contact row in [`SocialSnapshot::following`] — the user's NIP-02
/// (kind:3) follow list, projected for the iOS "Social" tab.
///
/// The shape is intentionally narrow: an avatar grid only needs the bech32
/// pubkey, a display name to surface under the avatar, and the picture URL.
/// Richer profile fields (NIP-05, NIP-39 external identities, lud16, …)
/// belong on a separate profile-detail projection so the grid stays cheap
/// to decode.
///
/// `npub` is pre-encoded so the iOS shell doesn't need a bech32 dependency
/// just to render the avatar fallback (truncated key).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ContactSummary {
    /// Author bech32 (`npub1…`) — pre-encoded so iOS can render the
    /// truncated-key fallback without a bech32 dep.
    pub npub: String,
    /// Cached display name from the contact's NIP-01 metadata, when
    /// known. `None` means the grid renders the truncated npub instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Cached avatar URL from the contact's NIP-01 metadata, when known.
    /// `None` means the grid renders the initial / fallback avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_url: Option<String>,
}

/// Snapshot of the user's Nostr social graph surfaced via
/// [`super::snapshot::PodcastUpdate::social`].
///
/// Mirrors the NIP-02 contact list (kind:3 follows) that the underlying
/// NMP substrate registers via `register_defaults`. For this PR the
/// projection layer still emits `None` — the contact store hook-up is
/// tracked in `docs/BACKLOG.md` (`pr-social-graph-nmp-store-wiring`) —
/// but the shape is fixed so the iOS shell can render against it as soon
/// as the data lands.
///
/// `following_count` is provided as a sugar so the UI can render the tab
/// badge without iterating `following`; it equals `following.len()` when
/// the projection is freshly built but stays correct even when callers
/// page through `following`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SocialSnapshot {
    /// Contacts the active account is following (NIP-02 kind:3 `p` tags).
    /// Empty when the contact list has been fetched but is genuinely
    /// empty; the field is `None` (not `Some([])`) when the projection
    /// layer hasn't fetched yet — see [`super::snapshot::PodcastUpdate`].
    pub following: Vec<ContactSummary>,
    /// Number of contacts on the active follow list. Equal to
    /// `following.len()` for now; surfaced separately so paged variants
    /// of `following` keep working without a second snapshot field.
    pub following_count: usize,
}

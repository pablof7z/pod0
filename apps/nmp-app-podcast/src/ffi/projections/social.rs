use serde::{Deserialize, Serialize};

use crate::store::friends::FriendRecord;

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

/// One turn within a [`NostrConversationDTO`].
///
/// `direction` is a plain string (`"inbound"` or `"outbound"`) rather than an
/// enum so the wire contract is forward-compatible with new directions without a
/// schema bump.  The iOS shell pattern-matches on the raw string.
///
/// **No explicit CodingKeys** — the bridge decoder uses `.convertFromSnakeCase`
/// so `event_id` → `eventId`, `created_at` → `createdAt`, etc.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct NostrConversationTurnDTO {
    /// Event id (lowercase hex) of the kind:1 note — stable identifier.
    pub event_id: String,
    /// `"inbound"` (from the peer) or `"outbound"` (published by the kernel).
    pub direction: String,
    /// Hex pubkey of the note's author. For inbound turns this is the
    /// counterparty; for outbound turns it is the active account.
    pub pubkey_hex: String,
    /// Unix seconds (`created_at` field of the kind:1 event).
    pub created_at: i64,
    /// Note body — the raw `content` field.
    pub content: String,
}

/// A NIP-10-threaded conversation between the active account and one peer,
/// surfaced via the `podcast.social` domain projection.
///
/// Conversations are keyed by the NIP-10 root event id.  Both inbound notes
/// (from the peer) and outbound turns (published by the kernel auto-responder)
/// are merged and ordered by `created_at` so the iOS/Android shell can render
/// a chat-bubble timeline without any client-side join.
///
/// **No explicit CodingKeys** — the bridge decoder uses `.convertFromSnakeCase`.
/// Critical mapping: `root_event_id` → `rootEventId` (lowercase `d`), matching
/// the Swift `NostrConversationRecord` merge path.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct NostrConversationDTO {
    /// NIP-10 root event id (lowercase hex) — the conversation key.
    pub root_event_id: String,
    /// Hex pubkey of the primary counterparty (the peer who opened the thread
    /// or who we most recently exchanged turns with).
    pub counterparty_hex: String,
    /// All unique participant hex pubkeys (active account + counterparty + any
    /// reply participants).  Always contains at least one entry.
    pub participants: Vec<String>,
    /// Merged inbound + outbound turns, sorted ascending by `created_at`.
    pub turns: Vec<NostrConversationTurnDTO>,
    /// Composed trust verdict for the primary counterparty:
    /// `(followed || approved) && !blocked`. Recomputed live — follow/unfollow,
    /// approve, and block immediately flip the verdict for all existing
    /// conversations (D6: fail-closed; `false` when no follow set is wired).
    pub trusted: bool,
    /// Whether the primary counterparty has an EXPLICIT block in the
    /// `ApprovedPeerStore`. Distinct from `trusted` (the composed verdict) so the
    /// shell can distinguish blocked-vs-untrusted and offer the correct recovery
    /// action (Unblock). Always present — bools are never omitted (D5
    /// omit-when-empty applies to Option/collections, not bools).
    pub peer_blocked: bool,
    /// Whether the primary counterparty has an EXPLICIT approval in the
    /// `ApprovedPeerStore`. EXPLICIT approval only — NOT follow-derived, so a
    /// pure-follow trusted peer reports `peer_approved = false`. Lets the shell
    /// avoid offering a no-op "Remove approval" on follow-only peers. Always
    /// present.
    pub peer_approved: bool,
    /// Unix seconds of the earliest turn in the conversation.
    pub first_seen: i64,
    /// Unix seconds of the most-recent turn in the conversation.
    pub last_activity: i64,
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
///
/// `pubkey_hex` is the raw lowercase-hex encoding of the same pubkey that
/// `npub` bech32-encodes.  Android calls `bridge.claimProfile(pubkeyHex)`
/// to trigger kind:0 resolution via the NMP `resolved_profiles` seam;
/// iOS decodes it via `convertFromSnakeCase` (pubkey_hex → pubkeyHex).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ContactSummary {
    /// Author bech32 (`npub1…`) — pre-encoded so iOS can render the
    /// truncated-key fallback without a bech32 dep.
    pub npub: String,
    /// Raw lowercase-hex encoding of the pubkey — required by Android's
    /// `bridge.claimProfile(pubkeyHex)` to trigger kind:0 profile resolution
    /// via the `resolved_profiles` seam.  Empty string only when the hex
    /// encoding fails (should never occur for a valid Nostr pubkey).
    pub pubkey_hex: String,
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
    /// Explicitly approved peer pubkeys from Rust's `ApprovedPeerStore`.
    /// Sorted lowercase hex. Native shells render this list but do not own it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approved_pubkeys: Vec<String>,
    /// Explicitly blocked peer pubkeys from Rust's `ApprovedPeerStore`.
    /// Sorted lowercase hex. Native shells render this list but do not own it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_pubkeys: Vec<String>,
}

/// One user-curated friend row projected from Rust-owned `FriendsState`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct FriendSummary {
    pub id: String,
    pub display_name: String,
    pub pubkey_hex: String,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

impl From<FriendRecord> for FriendSummary {
    fn from(friend: FriendRecord) -> Self {
        Self {
            id: friend.id,
            display_name: friend.display_name,
            pubkey_hex: friend.pubkey_hex,
            added_at: friend.added_at,
            avatar_url: friend.avatar_url,
            about: friend.about,
        }
    }
}

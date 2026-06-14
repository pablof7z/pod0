use serde::{Deserialize, Serialize};

/// Narrow identity projection surfaced via
/// [`super::snapshot::PodcastUpdate::active_account`].
///
/// Present when an identity is loaded; `None` while the kernel hasn't yet
/// resolved the active account (pre-sign-in or between identity switch and
/// the first snapshot tick).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AccountSummary {
    pub npub: String,
    /// Lowercase 64-hex pubkey. This is the canonical account id; `npub` is
    /// for display. Hosts must use this field for signing, profile lookup,
    /// filter construction, allowlists, and account removal.
    pub pubkey_hex: String,
    /// Short stable account fingerprint derived from SHA-256(pubkey bytes).
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_url: Option<String>,
}

/// Snapshot row for a podcast the user owns (has generated a NIP-F4
/// per-podcast keypair for via the `podcast.publish.create_owned_podcast`
/// action). Surfaced via [`super::snapshot::PodcastUpdate::owned_podcasts`].
///
/// `show_event_json` is the most recently constructed `kind:10154` event
/// (unsigned, for debug/diagnostic visibility) — the relay-publish path
/// is `relay_pending` until the broader Nostr publishing infrastructure
/// is wired through. `last_published_at` is Unix seconds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct OwnedPodcastInfo {
    pub podcast_id: String,
    pub podcast_pubkey_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_event_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_published_at: Option<i64>,
}

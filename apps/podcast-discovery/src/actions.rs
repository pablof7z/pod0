//! NIP-F4 action payloads.
//!
//! Stable string ids the iOS shell encodes alongside JSON payloads when
//! it dispatches a discovery/publishing action through the kernel. The
//! `ActionModule` impls that actually publish / subscribe arrive in M10.D;
//! M10.A only fixes the wire shape so the Swift bridge has a contract to
//! encode against (mirrors the M7.A `podcast-agent-core::actions` pattern).
//!
//! ## Wire shape
//!
//! ```text
//! podcast.nip74.publish_show     — PublishShowAction    { podcast_id }
//! podcast.nip74.publish_episode  — PublishEpisodeAction { episode_id }
//! podcast.nip74.discover         — DiscoverPodcastsAction
//!                                  { query?, limit?, relay_url? }
//! ```
//!
//! NIP-F4 discovery (`podcast.discover_nostr`) is routed via the
//! `PodcastAction::DiscoverNostr` variant in `nmp-app-podcast` — it
//! reuses the kernel's existing podcast namespace rather than declaring
//! a parallel set of `podcast.nip_f4.*` ids here, because the AddShowSheet
//! flow ultimately resolves to the existing `podcast.subscribe` RSS
//! pipeline (no separate publish path is required).

use serde::{Deserialize, Serialize};

/// `podcast.nip74.publish_show` — re-publish the agent-owned podcast's
/// `kind:10154` show event (NIP-F4) using the latest snapshot of the [`Podcast`]
/// row identified by `podcast_id`.
///
/// [`Podcast`]: podcast_core::Podcast
pub const ACTION_PUBLISH_SHOW: &str = "podcast.nip74.publish_show";

/// `podcast.nip74.publish_episode` — publish a `kind:54` event (NIP-F4) for an
/// existing episode. The action module is expected to look up the parent
/// podcast, upload audio/chapters/transcripts to Blossom (M10.B), then
/// build the tag set via [`crate::build::episode_to_episode_tags`].
pub const ACTION_PUBLISH_EPISODE: &str = "podcast.nip74.publish_episode";

/// `podcast.nip74.discover` — request a fresh discovery sweep of the
/// configured relays. Optional `query` narrows by title/category, and
/// optional `limit` caps the number of returned `NipF4DiscoveryShow`s.
pub const ACTION_DISCOVER_PODCASTS: &str = "podcast.nip74.discover";

/// Payload for [`ACTION_PUBLISH_SHOW`].
///
/// `podcast_id` is the UUID string of a `podcast_core::Podcast` row. The
/// action module resolves the row, derives the tag set from
/// [`crate::build::podcast_to_show_tags`], and signs with the agent
/// signer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishShowAction {
    pub podcast_id: String,
}

/// Payload for [`ACTION_PUBLISH_EPISODE`].
///
/// `episode_id` is the UUID string of a `podcast_core::Episode` row that
/// has already been recorded locally (Blossom upload may be triggered as
/// part of the dispatch).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishEpisodeAction {
    pub episode_id: String,
}

/// Payload for [`ACTION_DISCOVER_PODCASTS`].
///
/// Both fields are optional so the simplest dispatch — "show me what's
/// out there" — is a valid empty-object request. `relay_url` lets the UI
/// scope a sweep to a single relay (the Swift `NostrDiscoverForm` calls
/// fetch-shows with one relay at a time).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverPodcastsAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

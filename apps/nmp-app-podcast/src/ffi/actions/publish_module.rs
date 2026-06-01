//! Compound NIP-F4 publishing ActionModule (`podcast.publish` namespace).
//!
//! Routes every NIP-F4 owned-podcast publishing op: synthetic-row
//! creation (`create_synthetic_podcast`), per-podcast keypair
//! lifecycle (`create_owned_podcast` / `remove_owned_podcast`), show
//! event build (`publish_show`, kind:10154), episode event build
//! (`publish_episode`, kind:54), the agent-side ownership claim
//! (`publish_author_claim`, kind:10064), and the full owned-podcast
//! update/delete lifecycle (`update_owned_podcast`, `delete_owned_podcast`).
//!
//! All "publish" ops currently return
//! `{"ok": true, "status": "relay_pending", "event_tags": [...]}` and
//! stamp the constructed event JSON onto the handle's `publish_state`
//! map so the iOS shell can render "last built event" diagnostics. Real
//! relay-side publishing is wired through once the NMP Nostr relay
//! plumbing for per-app keypairs lands.
//!
//! ## D0 / D7
//!
//! The action module itself only routes — it serializes the typed
//! [`PublishAction`] back to JSON and emits a `DispatchHostOp`
//! `ActorCommand`. The host-op handler (running on the actor thread,
//! see [`crate::host_op_handler`]) owns the actual build + state mutation.
//! That layering matches `PodcastActionModule` / `PlayerActionModule`.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// `podcast.publish.create_synthetic_podcast` — insert a synthetic
/// (feed-less) podcast row into the kernel store from full metadata so
/// the Rust store is the single source of truth for owned podcasts.
/// Must run before `create_owned_podcast` (which only registers the key
/// and requires the row to already exist).
pub const ACTION_PUBLISH_CREATE_SYNTHETIC: &str = "podcast.publish.create_synthetic_podcast";

/// `podcast.publish.register_synthetic_episode` — insert an
/// agent-generated episode (from the TTS composer) under a synthetic
/// podcast so the kernel store is the source of truth. Without this the
/// episode lives only in the Swift render store and is wiped by the next
/// projection full-replace tick; with it, `publish_episode` can also find
/// the episode by id to publish via NIP-F4.
pub const ACTION_PUBLISH_REGISTER_SYNTHETIC_EPISODE: &str =
    "podcast.publish.register_synthetic_episode";

/// `podcast.publish.create_owned_podcast` — generate a per-podcast
/// secret key, derive the pubkey, write `owner_pubkey_hex` back onto
/// the `Podcast` row.
pub const ACTION_PUBLISH_CREATE_OWNED: &str = "podcast.publish.create_owned_podcast";

/// `podcast.publish.update_owned_podcast` — mutate the owned podcast's
/// metadata in the kernel store and (when public + nostr-enabled)
/// re-publish the `kind:10154` show event. Swift no longer triggers a
/// separate publish after updating.
pub const ACTION_PUBLISH_UPDATE_OWNED: &str = "podcast.publish.update_owned_podcast";

/// `podcast.publish.delete_owned_podcast` — publish a NIP-09 (kind:5)
/// deletion for the show event, drop the per-podcast key, and remove the
/// podcast row + episodes from the kernel store.
pub const ACTION_PUBLISH_DELETE_OWNED: &str = "podcast.publish.delete_owned_podcast";

/// `podcast.publish.publish_show` — build a `kind:10154` show event
/// from the podcast row + its per-podcast keypair.
pub const ACTION_PUBLISH_PUBLISH_SHOW: &str = "podcast.publish.publish_show";

/// `podcast.publish.publish_episode` — build a `kind:54` episode event
/// from the episode row + its podcast's per-podcast keypair.
pub const ACTION_PUBLISH_PUBLISH_EPISODE: &str = "podcast.publish.publish_episode";

/// `podcast.publish.publish_author_claim` — build a `kind:10064`
/// author-claim event listing every owned podcast pubkey under the
/// supplied agent pubkey.
pub const ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM: &str = "podcast.publish.publish_author_claim";

/// `podcast.publish.remove_owned_podcast` — drop the per-podcast key
/// pair + clear `owner_pubkey_hex`.
pub const ACTION_PUBLISH_REMOVE_OWNED: &str = "podcast.publish.remove_owned_podcast";

/// Wire enum for every `podcast.publish.*` action.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` matches the
/// dispatch shape Swift already uses for `PodcastAction` /
/// `PlayerAction` — the iOS shell encodes
/// `{"op":"publish_show","podcast_id":"…"}` and the action module
/// dispatches the variant to the host-op handler.
/// One chapter for a `RegisterSyntheticEpisode` op. `image_url` +
/// `source_episode_id` carry the parity fields the Swift TTS composer built on
/// `Episode.Chapter` (mid-play artwork swap + source-episode chip). They round
/// the kernel store, not just the wire, so the projected chapter is identical
/// to the pre-kernel build.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SyntheticChapterArg {
    pub start_secs: f64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PublishAction {
    /// Insert a synthetic (feed-less) podcast row from full metadata.
    /// `podcast_id` is the Swift-minted UUID string so both stores agree
    /// on identity. `visibility` is the canonical `NostrVisibility`
    /// snake_case string (`"public"` / `"private"`).
    CreateSyntheticPodcast {
        podcast_id: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        author: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artwork_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default)]
        categories: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<String>,
    },
    /// Insert an agent-generated episode under a synthetic podcast so the
    /// kernel store owns it (survives the projection full-replace tick).
    /// `podcast_id` / `episode_id` are the Swift-minted UUID strings.
    /// `audio_path` is the local file path of the stitched m4a (Swift still
    /// owns the file write). `chapters` carry the parity fields; `transcript`
    /// is the flat episode transcript text.
    RegisterSyntheticEpisode {
        podcast_id: String,
        episode_id: String,
        title: String,
        audio_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_secs: Option<f64>,
        #[serde(default)]
        chapters: Vec<SyntheticChapterArg>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
    },
    CreateOwnedPodcast {
        podcast_id: String,
    },
    /// Update mutable metadata on an owned podcast. `None` fields keep the
    /// current value (partial update). Re-publishes the show event when the
    /// podcast is public + nostr is enabled (the kernel owns that gate).
    /// `author` + `visibility` are carried so the kernel store stays the SSOT
    /// (otherwise the next snapshot push reverts a Swift-side edit / flip).
    /// `visibility` is the canonical `NostrVisibility` snake_case string.
    UpdateOwnedPodcast {
        podcast_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artwork_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<String>,
    },
    PublishShow {
        podcast_id: String,
    },
    PublishEpisode {
        episode_id: String,
    },
    PublishAuthorClaim {
        agent_pubkey_hex: String,
    },
    /// Full deletion lifecycle: NIP-09 deletion event → drop key → remove
    /// row. Supersedes `RemoveOwnedPodcast` as the canonical delete path.
    DeleteOwnedPodcast {
        podcast_id: String,
    },
    RemoveOwnedPodcast {
        podcast_id: String,
    },
}

/// Single action module for the `podcast.publish` namespace.
pub struct NipF4PublishModule;

impl ActionModule for NipF4PublishModule {
    const NAMESPACE: &'static str = "podcast.publish";

    type Action = PublishAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json = serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "publish_module_tests.rs"]
mod tests;

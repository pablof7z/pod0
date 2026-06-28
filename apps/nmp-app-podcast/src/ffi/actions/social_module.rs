//! Compound `"podcast.social"` ActionModule — routes user-identity social
//! publishing (kind:0 profile, kind:1 note, kind:9802 NIP-84 highlight)
//! into the actor thread where [`crate::social_publish_handler`] reads the
//! active signing key from `IdentityStore`, signs the event, and broadcasts
//! it through the Nostr relay capability.
//!
//! Per D7 the kernel owns the signing policy. The action module is pure
//! routing: Swift encodes
//! `{"op":"publish_profile","name":"...","display_name":"...",...}` /
//! `{"op":"publish_note","content":"...","episode_coord":"30311:..."}` /
//! `{"op":"publish_highlight","content":"...","enclosure_url":"...","feed_url":"...","item_guid":"...","start_sec":12,"end_sec":34,"caption":"..."}`
//! and the kernel handler assembles the NIP-73 / NIP-84 tags from the typed
//! fields (Swift passes semantic values, Rust builds the tags).
//!
//! ## Wire-contract note
//!
//! Unlike `podcast.identity` (which carries the legacy `#[serde(tag =
//! "type")]` PascalCase discriminator from before the `op` convention was
//! settled), this module uses the canonical `#[serde(tag = "op",
//! rename_all = "snake_case")]` shape that every newer namespace
//! (`podcast.inbox`, `podcast.publish`, …) shares. The host-op routing is a
//! `serde_json::from_str` waterfall keyed on the *tag value*, so the
//! `publish_profile` / `publish_note` / `publish_highlight` op strings —
//! all unique across the registered enums — match only this enum.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

use crate::store::notes::NoteTarget;

/// `podcast.social.publish_profile` — sign + publish a kind:0 profile.
pub const ACTION_SOCIAL_PUBLISH_PROFILE: &str = "podcast.social.publish_profile";
/// `podcast.social.publish_note` — sign + publish a kind:1 text note.
pub const ACTION_SOCIAL_PUBLISH_NOTE: &str = "podcast.social.publish_note";
/// `podcast.social.publish_highlight` — sign + publish a kind:9802 highlight.
pub const ACTION_SOCIAL_PUBLISH_HIGHLIGHT: &str = "podcast.social.publish_highlight";
/// `podcast.social.approve_peer` — add a pubkey to the kernel approve list.
pub const ACTION_SOCIAL_APPROVE_PEER: &str = "podcast.social.approve_peer";
/// `podcast.social.block_peer` — add a pubkey to the kernel block list.
pub const ACTION_SOCIAL_BLOCK_PEER: &str = "podcast.social.block_peer";
/// `podcast.social.remove_approval` — remove an explicit approval.
pub const ACTION_SOCIAL_REMOVE_APPROVAL: &str = "podcast.social.remove_approval";
/// `podcast.social.remove_block` — remove an explicit block.
pub const ACTION_SOCIAL_REMOVE_BLOCK: &str = "podcast.social.remove_block";
/// `podcast.social.add_note` — add a Rust-owned local note.
pub const ACTION_SOCIAL_ADD_NOTE: &str = "podcast.social.add_note";
/// `podcast.social.update_note` — update a Rust-owned local note.
pub const ACTION_SOCIAL_UPDATE_NOTE: &str = "podcast.social.update_note";
/// `podcast.social.delete_note` — mark a Rust-owned local note deleted.
pub const ACTION_SOCIAL_DELETE_NOTE: &str = "podcast.social.delete_note";
/// `podcast.social.restore_note` — restore a soft-deleted local note.
pub const ACTION_SOCIAL_RESTORE_NOTE: &str = "podcast.social.restore_note";
/// `podcast.social.clear_notes` — mark all Rust-owned local notes deleted.
pub const ACTION_SOCIAL_CLEAR_NOTES: &str = "podcast.social.clear_notes";

/// Wire enum for all `"podcast.social"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SocialAction {
    /// Sign + publish a kind:0 metadata event. `name` is required; the
    /// remaining fields are omitted from the JSON content when absent.
    PublishProfile {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        about: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        picture: Option<String>,
    },
    /// Sign + publish a kind:1 text note. The kernel builds the tags: a
    /// `["t","note"]` marker plus an optional `["a", episode_coord]` tag when
    /// `episode_coord` (a `30311:<author>:<id>` reference) is present.
    PublishNote {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        episode_coord: Option<String>,
    },
    /// Sign + publish a kind:9802 NIP-84 highlight. The kernel assembles the
    /// NIP-73 / NIP-84 tag set from these typed fields: `["r", enclosure_url]`
    /// + `["r", feed_url]` source refs, an `["i", "podcast:item:guid:<guid>#t=
    /// <start_sec>,<end_sec>"]` external content id, a `["context", content]`
    /// tag, and an optional `["alt", caption]`.
    PublishHighlight {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enclosure_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feed_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_guid: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_sec: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_sec: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    /// Add `pubkey_hex` to the kernel approve list. Clears any existing block
    /// for the same key. The kernel persists the store and re-emits
    /// `podcast.social` with updated `trusted` verdicts on the next tick.
    ApprovePeer {
        pubkey_hex: String,
    },
    /// Add `pubkey_hex` to the kernel block list. Clears any existing approval
    /// for the same key. Block is an absolute override of follow status.
    BlockPeer {
        pubkey_hex: String,
    },
    /// Remove an explicit approval for `pubkey_hex`. The peer reverts to
    /// follow-only trust.
    RemoveApproval {
        pubkey_hex: String,
    },
    /// Remove an explicit block for `pubkey_hex`. The peer reverts to
    /// follow-only trust.
    RemoveBlock {
        pubkey_hex: String,
    },
    /// Add a local note to the Rust-owned notes store.
    AddNote {
        id: String,
        text: String,
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<NoteTarget>,
        created_at: i64,
        author: String,
    },
    /// Update local note fields. Omitted fields are left unchanged.
    UpdateNote {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<NoteTarget>,
    },
    /// Soft-delete a local note.
    DeleteNote {
        id: String,
    },
    /// Restore a soft-deleted local note.
    RestoreNote {
        id: String,
    },
    /// Soft-delete every local note.
    ClearNotes,
}

/// `ActionModule` for the `"podcast.social"` namespace.
///
/// `execute` serializes the typed [`SocialAction`] back to JSON and hands
/// it to the actor thread as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` decodes it and routes into
/// [`crate::social_publish_handler`].
pub struct SocialActionModule;

impl ActionModule for SocialActionModule {
    const NAMESPACE: &'static str = "podcast.social";

    type Action = SocialAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "social_module_tests.rs"]
mod tests;

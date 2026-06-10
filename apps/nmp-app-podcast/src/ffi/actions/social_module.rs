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
use nmp_core::ActorCommand;

/// `podcast.social.publish_profile` — sign + publish a kind:0 profile.
pub const ACTION_SOCIAL_PUBLISH_PROFILE: &str = "podcast.social.publish_profile";
/// `podcast.social.publish_note` — sign + publish a kind:1 text note.
pub const ACTION_SOCIAL_PUBLISH_NOTE: &str = "podcast.social.publish_note";
/// `podcast.social.publish_highlight` — sign + publish a kind:9802 highlight.
pub const ACTION_SOCIAL_PUBLISH_HIGHLIGHT: &str = "podcast.social.publish_highlight";

/// Wire enum for all `"podcast.social"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
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
#[path = "social_module_tests.rs"]
mod tests;

//! Clip-action `ActionModule` — routes all `"podcast.clip.*"` dispatches.
//!
//! Swift encodes every clip action as `{"op":"<variant>", ...fields}`. The
//! `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps the
//! string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can mutate the
//! shared `Vec<ClipRecord>` via [`crate::clip_handler::ClipHandler`]
//! without the kernel naming podcast-domain nouns (D0).
//!
//! ## Wire shape
//!
//! ```text
//! podcast.clip.create     { episode_id, start_secs, end_secs, title?, source?, transcript_text?, client_clip_id? }
//! podcast.clip.delete     { clip_id }
//! podcast.clip.auto_snip  { episode_id, position_secs, source?, client_clip_id? }
//! podcast.clip.resolve_quote { episode_id, position_secs }
//! ```
//!
//! `create` and `auto_snip` return `{"ok":true,"clip_id":"<uuid>"}`;
//! `resolve_quote` returns non-persisted transcript-aligned quote bounds;
//! `delete` returns `{"ok":true}` (success even when the id is unknown —
//! idempotent delete).

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// `podcast.clip.create` — create a user-defined clip from `[start, end]`.
pub const ACTION_CLIP_CREATE: &str = "podcast.clip.create";
/// `podcast.clip.delete` — remove a previously-created clip by id.
pub const ACTION_CLIP_DELETE: &str = "podcast.clip.delete";
/// `podcast.clip.auto_snip` — create a clip around `position_secs`, refined to
/// transcript segment boundaries when timed transcript entries are available.
pub const ACTION_CLIP_AUTO_SNIP: &str = "podcast.clip.auto_snip";
/// `podcast.clip.resolve_quote` — return non-persistent quote bounds around
/// `position_secs`, refined to timed transcript segment boundaries.
pub const ACTION_CLIP_RESOLVE_QUOTE: &str = "podcast.clip.resolve_quote";

/// Wire enum for all `"podcast.clip"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ClipAction {
    Create {
        episode_id: String,
        start_secs: f64,
        end_secs: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript_text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_clip_id: Option<String>,
    },
    Delete {
        clip_id: String,
    },
    AutoSnip {
        episode_id: String,
        position_secs: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_clip_id: Option<String>,
    },
    ResolveQuote {
        episode_id: String,
        position_secs: f64,
    },
}

/// Action module for the `"podcast.clip"` namespace.
pub struct ClipActionModule;

impl ActionModule for ClipActionModule {
    const NAMESPACE: &'static str = "podcast.clip";

    type Action = ClipAction;

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
#[path = "clip_module_tests.rs"]
mod tests;

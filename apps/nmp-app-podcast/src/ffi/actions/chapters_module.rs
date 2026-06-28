//! AI chapter `ActionModule` — routes all `"podcast.chapters.*"` dispatches.
//!
//! Sibling to [`super::player_module`] / [`super::podcast_module`]; lives
//! in its own namespace so the iOS shell can dispatch
//! `podcast.chapters.compile` literally (matching the legacy
//! `App/Sources/Services/AIChapterCompiler.swift` mental model) without
//! piggy-backing on the broader `podcast.*` action enum.
//!
//! The kernel-side body of `compile` lives in [`crate::ai_chapters`]; this
//! file is pure routing (D7 — the action module decides nothing).
//!
//! ## Wire shape
//!
//! `podcast.chapters.compile { episode_id }` — synthesize transcript-grounded
//! chapters from the cached transcript for `episode_id`. Returns
//! `{"ok":true,"status":"compiling","episode_id":<id>}` on success,
//! `{"ok":true,"status":"already_has_chapters"}` when the episode
//! already has chapters (RSS or prior compile), or
//! `{"ok":false,"error":"no_transcript"|"no_duration"|…}` on the
//! gate-failure cases. See [`crate::ai_chapters::handle_compile_chapters`].

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.chapters"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChaptersAction {
    /// Synthesize AI chapters for an episode that has a cached
    /// transcript but no RSS / Podcasting 2.0 chapters yet.
    Compile { episode_id: String },
}

/// Action module for the `"podcast.chapters"` namespace.
pub struct ChaptersActionModule;

impl ActionModule for ChaptersActionModule {
    const NAMESPACE: &'static str = "podcast.chapters";

    type Action = ChaptersAction;

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
#[path = "chapters_module_tests.rs"]
mod tests;

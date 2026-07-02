//! Compound inbox ActionModule — routes all `"podcast.inbox.*"` dispatches.
//!
//! The inbox is the "what should I listen to next" projection: every
//! unlistened episode across the user's whole library, minus the set
//! the user has explicitly dismissed, sorted by a heuristic priority
//! score. The score is computed in [`super::super::super::inbox_handler`]
//! on the actor thread and projected through
//! [`super::super::projections::InboxItem`] every snapshot tick.
//!
//! Per D7 the kernel owns the policy. The action module is pure routing:
//! Swift encodes `{"op":"triage"}` / `{"op":"dismiss","episode_id":"..."}`
//! / `{"op":"mark_listened","episode_id":"..."}` and the handler does the
//! work. There are no LLM calls — the current heuristic (recency-weighted)
//! is intentionally a stub that a later milestone will swap for a real
//! classifier without changing the wire contract.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.inbox"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `triage` → `{"op":"triage"}`,
/// `dismiss` → `{"op":"dismiss","episode_id":"..."}`,
/// `mark_listened` → `{"op":"mark_listened","episode_id":"..."}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum InboxAction {
    /// Recompute the inbox projection (bumps `rev` so the next snapshot
    /// tick rebuilds the `inbox` field). The inbox itself is rebuilt
    /// every tick from the store + dismissed set, so this is effectively
    /// a "force re-render" signal — useful when the user pulls to refresh
    /// and we want a visible UI tick even when nothing else has changed.
    Triage,
    /// Mark an episode as dismissed from the inbox. Stored in-memory on
    /// `PodcastHandle.dismissed_episode_ids`; survives until the kernel
    /// is torn down (no persistence per the M2.E inbox scope).
    Dismiss { episode_id: String },
    /// Mark an episode as listened (`Episode.played = true`). Persists
    /// through the store, so the row falls out of the inbox on the next
    /// tick.
    MarkListened { episode_id: String },
    /// Revert an accidental mark-played (`Episode.played = false`). Persists
    /// through the store, so the row re-enters the inbox on the next tick.
    MarkUnlistened { episode_id: String },
}

/// `ActionModule` for the `"podcast.inbox"` namespace.
///
/// `execute` serializes the typed [`InboxAction`] back to JSON and hands
/// it to the actor thread as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` decodes it and dispatches into the
/// `inbox_handler` module.
pub struct InboxActionModule;

impl ActionModule for InboxActionModule {
    const NAMESPACE: nmp_core::substrate::DeclaredActionNamespace =
        nmp_core::substrate::DeclaredActionNamespace::app_owned("podcast.inbox");

    type Action = InboxAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        _ctx: &nmp_core::substrate::ActionContext,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE.as_str(), &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "inbox_module_tests.rs"]
mod tests;

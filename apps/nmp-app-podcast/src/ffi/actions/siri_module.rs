//! Siri-action `ActionModule` — routes all `"podcast.siri.*"` dispatches.
//!
//! Episode-selection policy lives here in Rust (D0, D7): Swift only names
//! the intent, the kernel decides which episode to play.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.siri"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SiriAction {
    /// Play the latest unplayed episode from the whole library, or from a
    /// specific podcast when `podcast_id` is supplied.
    PlayLatest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        podcast_id: Option<String>,
    },
    /// Resume the episode that was last playing. If no episode is loaded,
    /// falls back to the same selection as `PlayLatest`.
    Resume,
}

/// Action module for the `"podcast.siri"` namespace.
pub struct SiriActionModule;

impl ActionModule for SiriActionModule {
    const NAMESPACE: nmp_core::substrate::DeclaredActionNamespace =
        nmp_core::substrate::DeclaredActionNamespace::app_owned("podcast.siri");

    type Action = SiriAction;

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
#[path = "siri_module_tests.rs"]
mod tests;

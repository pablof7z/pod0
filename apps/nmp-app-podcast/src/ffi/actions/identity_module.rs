//! `podcast.identity` action module — routes all identity dispatches into
//! the actor thread where `PodcastHostOpHandler` can mutate the shared
//! `IdentityStore` and bump `rev`.

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

// Re-export the action enum so callers that parse raw JSON can import it from
// one place alongside the module struct.
pub use crate::identity_handler::IdentityAction;

/// Single action module for the whole `"podcast.identity"` namespace.
pub struct IdentityActionModule;

impl ActionModule for IdentityActionModule {
    const NAMESPACE: &'static str = "podcast.identity";

    type Action = IdentityAction;

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

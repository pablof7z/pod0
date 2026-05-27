//! `podcast.identity` action module — routes all identity dispatches into
//! the actor thread where `PodcastHostOpHandler` can mutate the shared
//! `IdentityStore` and bump `rev`.

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

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
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json =
            serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

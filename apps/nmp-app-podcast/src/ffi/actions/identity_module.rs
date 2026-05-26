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

// ---------------------------------------------------------------------------
// IdentityAction must impl Serialize for the ActionModule's execute body.
// We add a minimal Serialize derive-compatible impl via a newtype or manually.
// ---------------------------------------------------------------------------

// We need IdentityAction to be Serialize so ActionModule::execute can
// re-encode it. Add a custom Serialize impl here.
impl serde::Serialize for IdentityAction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            IdentityAction::ImportNsec { nsec } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "ImportNsec")?;
                map.serialize_entry("nsec", nsec)?;
                map.end()
            }
            IdentityAction::Generate => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "Generate")?;
                map.end()
            }
            IdentityAction::Clear => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "Clear")?;
                map.end()
            }
            IdentityAction::FetchProfile => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "FetchProfile")?;
                map.end()
            }
        }
    }
}

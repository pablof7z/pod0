//! `SocialAction` dispatch (user-identity kind:0/1/9802 publishing)
//! extracted from `host_op_handler.rs` to keep that file under the 500-LOC
//! hard ceiling (AGENTS.md). The `handle_social_action` method stays on
//! `PodcastHostOpHandler` via this sibling `impl` block.
//!
//! Each op reads the active local signing key from `IdentityStore`, signs
//! the event in `crate::social_publish_handler`, and broadcasts it (or
//! returns `{"status":"signed"}` under a null app pointer in unit tests).

use crate::ffi::actions::social_module::SocialAction;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    pub(crate) fn handle_social_action(
        &self,
        action: SocialAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            SocialAction::PublishProfile {
                name,
                display_name,
                about,
                picture,
            } => crate::social_publish_handler::handle_publish_profile(
                self.app,
                &self.identity,
                &name,
                display_name.as_deref(),
                about.as_deref(),
                picture.as_deref(),
                correlation_id,
            ),
            SocialAction::PublishNote { content, tags } => {
                crate::social_publish_handler::handle_publish_note(
                    self.app,
                    &self.identity,
                    &content,
                    tags.as_ref(),
                    correlation_id,
                )
            }
            SocialAction::PublishHighlight { content, tags } => {
                crate::social_publish_handler::handle_publish_highlight(
                    self.app,
                    &self.identity,
                    &content,
                    tags.as_ref(),
                    correlation_id,
                )
            }
        }
    }
}

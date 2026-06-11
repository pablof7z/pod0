//! `HostOpHandler` impl for `PodcastHostOpHandler`.
//!
//! Extracted here to keep `host_op_handler.rs` under the 500-line hard limit
//! (AGENTS.md). The routing logic is its own coherent unit: peel the
//! namespaced envelope, match on `ns`, parse the `action` field against the
//! exactly-one enum that owns that namespace, and forward to the per-domain
//! `handle_*` helper.
//!
//! ## Why a namespaced envelope?
//!
//! Prior to this change the cascade tried ~19 `serde_json::from_str` attempts
//! in source order, taking the first that deserialised.  Because the enums use
//! `#[serde(tag="op")]` without `deny_unknown_fields`, identical/subset wire
//! shapes collide.  Five confirmed silent misroutes resulted:
//!
//! * `podcast.knowledge.search` `{"op":"search","query":..}` → hijacked by `WikiAction::Search`
//! * `podcast.siri.resume` `{"op":"resume"}` → hijacked by `PlayerAction::Resume`
//! * `podcast.voice.stop` `{"op":"stop"}` → hijacked by `PlayerAction::Stop`
//! * `podcast.agent.clear` `{"op":"clear"}` → hijacked by `QueueAction`
//! * `podcast.player.download` → hijacked by `PodcastAction::Download`
//!
//! The envelope `{"ns":"<M::NAMESPACE>","action":<bare action>}` is produced by
//! every `ActionModule::execute` body (see `crate::ffi::actions::dispatch_host_op`)
//! and consumed only here.  It is never persisted or replayed, so the wire-shape
//! change requires no migration and no upstream nmp-core change.

use nmp_core::substrate::HostOpHandler;

use super::PodcastHostOpHandler;
use crate::ai_chapters::{handle_compile_chapters, handle_compile_chapters_with_signal};
use crate::ffi::actions::agent_module::AgentChatAction;
use crate::ffi::actions::categorization_module::CategorizationAction;
use crate::ffi::actions::chapters_module::ChaptersAction;
use crate::ffi::actions::clip_module::ClipAction;
use crate::ffi::actions::identity_module::IdentityAction;
use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::actions::memory_module::MemoryAction;
use crate::ffi::actions::picks_module::PicksAction;
use crate::ffi::actions::player_module::PlayerAction;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::ffi::actions::publish_module::PublishAction;
use crate::ffi::actions::queue_module::QueueAction;
use crate::ffi::actions::settings_module::SettingsAction;
use crate::ffi::actions::siri_module::SiriAction;
use crate::ffi::actions::social_module::SocialAction;
use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::actions::voice_module::VoiceAction;
use crate::ffi::actions::wiki_module::WikiAction;
use crate::host_op_handler_queue::handle_queue_action;
use crate::host_op_publish::handle_publish_action;
use crate::identity_handler::IdentityHandler;
use crate::inbox_handler::{handle_inbox_action, handle_inbox_action_with_signal};
use crate::memory_handler;
use crate::voice_handler;

/// Namespaced envelope produced by every `ActionModule::execute` body via
/// [`crate::ffi::actions::dispatch_host_op`].
/// Shape: `{"ns":"<namespace>","action":<bare action value>}`.
#[derive(serde::Deserialize)]
struct HostOpEnvelope {
    ns: String,
    action: serde_json::Value,
}

impl HostOpHandler for PodcastHostOpHandler {
    /// Route a host-op by peeling the `ns` key from the envelope, then
    /// parsing `action` against exactly the one enum that owns that namespace.
    /// Unknown namespaces and parse failures are observable (warn-logged +
    /// `{"ok":false}`) instead of silently falling through to a wrong handler.
    fn handle(&self, action_json: &str, correlation_id: &str) -> serde_json::Value {
        let env = match serde_json::from_str::<HostOpEnvelope>(action_json) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("host_op: malformed envelope ({}): {}", e, action_json);
                return serde_json::json!({"ok": false, "error": format!("malformed envelope: {e}")});
            }
        };

        macro_rules! parse {
            ($T:ty) => {
                match serde_json::from_value::<$T>(env.action.clone()) {
                    Ok(a) => a,
                    Err(e) => {
                        log::warn!(
                            "host_op: failed to parse action for ns={} ({}): {}",
                            env.ns, e, action_json
                        );
                        return serde_json::json!({"ok": false, "error": format!("parse error for ns={}: {e}", env.ns)});
                    }
                }
            };
        }

        match env.ns.as_str() {
            "podcast.identity" => {
                let action = parse!(IdentityAction);
                let mut handler =
                    IdentityHandler::new(self.identity.clone(), self.rev.clone());
                if let Some(ref signal) = self.snapshot_signal {
                    handler = handler.with_snapshot_signal(signal.clone());
                }
                handler.handle(action)
            }
            "podcast.categorize" => self.state.categories.handle(parse!(CategorizationAction)),
            "podcast" => self.handle_podcast_action(parse!(PodcastAction), correlation_id),
            "podcast.publish" => handle_publish_action(self, parse!(PublishAction)),
            "podcast.player" => {
                self.handle_player_action(parse!(PlayerAction), correlation_id)
            }
            "podcast.inbox" => {
                let action = parse!(InboxAction);
                if let Some(signal) = self.snapshot_signal.clone() {
                    handle_inbox_action_with_signal(
                        action,
                        &self.store,
                        &self.dismissed_episode_ids,
                        &self.rev,
                        &self.inbox_triage_cache,
                        &self.runtime,
                        &self.inbox_triage_in_progress,
                        signal,
                    )
                } else {
                    handle_inbox_action(
                        action,
                        &self.store,
                        &self.dismissed_episode_ids,
                        &self.rev,
                        &self.inbox_triage_cache,
                        &self.runtime,
                        &self.inbox_triage_in_progress,
                    )
                }
            }
            "podcast.queue" => {
                handle_queue_action(&self.queue, &self.store, &self.rev, parse!(QueueAction))
            }
            "podcast.chapters" => {
                let action = parse!(ChaptersAction);
                match action {
                    ChaptersAction::Compile { episode_id } => {
                        if let Some(signal) = self.snapshot_signal.clone() {
                            handle_compile_chapters_with_signal(
                                &self.store,
                                &self.rev,
                                &self.runtime,
                                episode_id,
                                signal,
                            )
                        } else {
                            handle_compile_chapters(
                                &self.store,
                                &self.rev,
                                &self.runtime,
                                episode_id,
                            )
                        }
                    }
                }
            }
            "podcast.wiki" => self.state.wiki.handle(parse!(WikiAction)),
            "podcast.picks" => self.state.picks.handle(parse!(PicksAction)),
            "podcast.tasks" => self.state.tasks.handle(parse!(AgentTasksAction), self.app),
            "podcast.knowledge" => self.state.knowledge.handle(parse!(KnowledgeAction)),
            "podcast.memory" => {
                memory_handler::handle(parse!(MemoryAction), &self.store, &self.rev)
            }
            "podcast.clip" => self.state.clips.handle(parse!(ClipAction)),
            "podcast.voice" => {
                voice_handler::handle(self, parse!(VoiceAction), correlation_id)
            }
            "podcast.agent" => self.agent_chat.handle(parse!(AgentChatAction)),
            "podcast.settings" => self.handle_settings_action(parse!(SettingsAction)),
            "podcast.siri" => self.handle_siri_action(parse!(SiriAction), correlation_id),
            "podcast.social" => {
                self.handle_social_action(parse!(SocialAction), correlation_id)
            }
            ns => {
                log::warn!("host_op: unknown namespace {:?}: {}", ns, action_json);
                serde_json::json!({"ok": false, "error": format!("unknown namespace: {ns}")})
            }
        }
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod router_tests;

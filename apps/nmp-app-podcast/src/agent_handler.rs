//! Agent-chat action handler — owns the in-memory conversation transcript
//! that `super::host_op_handler::PodcastHostOpHandler` routes
//! `podcast.agent.*` dispatches into.
//!
//! Extracted into its own module so `host_op_handler.rs` stays under the
//! 500-line hard limit (it already sits at 499 LOC on `main`). The shape
//! of this module deliberately mirrors the inline handler methods on
//! `PodcastHostOpHandler`: a small struct that holds the shared `Arc`s and
//! exposes one entry point per action variant, returning the
//! `{"ok":true}` envelope shape every action handler in this crate uses.
//!
//! PR 6 replaces the scaffold canned reply with a real synchronous Ollama
//! call via `agent_llm::chat_sync`. When Ollama is unreachable the handler
//! falls back to `SCAFFOLD_ASSISTANT_REPLY` so the UI always receives
//! a non-empty assistant message.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::agent_llm;
use crate::ffi::actions::AgentChatAction;
use crate::ffi::projections::AgentMessageSummary;

/// Owns the agent-chat conversation transcript and the `is_busy` /
/// "touched" flags. Held by `super::host_op_handler::PodcastHostOpHandler`
/// alongside the other domain handlers (audio, downloads, …) so all
/// state mutations happen on the actor thread.
pub struct AgentChatHandler {
    conversation: Arc<Mutex<Vec<AgentMessageSummary>>>,
    busy: Arc<AtomicBool>,
    touched: Arc<AtomicBool>,
    rev: Arc<AtomicU64>,
    /// Shared Tokio runtime for the blocking LLM call.
    /// `None` in unit tests (where the runtime isn't wired in) so the handler
    /// falls back to the scaffold reply without attempting a network connection.
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

/// Fallback assistant reply used when Ollama is offline or the model errors.
/// Also used in unit tests that don't supply a Tokio runtime.
pub const SCAFFOLD_ASSISTANT_REPLY: &str = "I'm thinking about your question…";

/// System prompt for agent-chat turns.
const AGENT_SYSTEM_PROMPT: &str =
    "You are a helpful podcast assistant. Answer questions about podcasts, episodes, \
     RSS feeds, and related topics concisely and accurately.";

impl AgentChatHandler {
    /// Create a handler with a live Tokio runtime (production path).
    pub fn new(
        conversation: Arc<Mutex<Vec<AgentMessageSummary>>>,
        busy: Arc<AtomicBool>,
        touched: Arc<AtomicBool>,
        rev: Arc<AtomicU64>,
        runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self { conversation, busy, touched, rev, runtime: Some(runtime) }
    }

    /// Create a handler without a runtime (test / scaffold path).
    /// All `Send` calls fall back to `SCAFFOLD_ASSISTANT_REPLY`.
    pub fn new_without_runtime(
        conversation: Arc<Mutex<Vec<AgentMessageSummary>>>,
        busy: Arc<AtomicBool>,
        touched: Arc<AtomicBool>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self { conversation, busy, touched, rev, runtime: None }
    }

    /// Route a typed [`AgentChatAction`] to the right entry point.
    pub fn handle(&self, action: AgentChatAction) -> serde_json::Value {
        match action {
            AgentChatAction::Send { message } => self.handle_send(message),
            AgentChatAction::Clear => self.handle_clear(),
        }
    }

    fn handle_send(&self, message: String) -> serde_json::Value {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return serde_json::json!({"ok": false, "error": "empty message"});
        }
        let now = Utc::now().timestamp();

        // Read the current history snapshot WITHOUT holding the mutex across the LLM call.
        let history_snapshot: Vec<(String, String)> = match self.conversation.lock() {
            Ok(c) => c.iter().map(|m| (m.role.clone(), m.content.clone())).collect(),
            Err(_) => return serde_json::json!({"ok": false, "error": "conversation poisoned"}),
        };

        // Prepare the user message and a placeholder assistant row.
        let user_msg = AgentMessageSummary {
            id: Uuid::new_v4().to_string(),
            role: "user".to_owned(),
            content: trimmed.to_owned(),
            created_at: now,
            is_generating: false,
        };
        let assistant_placeholder = AgentMessageSummary {
            id: Uuid::new_v4().to_string(),
            role: "assistant".to_owned(),
            content: String::new(),
            created_at: now,
            is_generating: true,
        };

        // Push user message and placeholder; mark busy.
        match self.conversation.lock() {
            Ok(mut c) => {
                c.push(user_msg);
                c.push(assistant_placeholder);
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "conversation poisoned"}),
        }
        self.busy.store(true, Ordering::Relaxed);
        self.touched.store(true, Ordering::Relaxed);
        self.rev.fetch_add(1, Ordering::Relaxed);

        // Call the LLM synchronously (actor thread is a plain std::thread, block_on is safe).
        // Fall back to the scaffold reply on any error.
        let reply = match &self.runtime {
            Some(rt) => agent_llm::chat_sync(
                AGENT_SYSTEM_PROMPT,
                &history_snapshot,
                trimmed,
                rt,
            )
            .unwrap_or_else(|_| SCAFFOLD_ASSISTANT_REPLY.to_owned()),
            None => SCAFFOLD_ASSISTANT_REPLY.to_owned(),
        };

        // Mutate the placeholder in-place: fill content and clear is_generating.
        match self.conversation.lock() {
            Ok(mut c) => {
                if let Some(last) = c.last_mut() {
                    last.content = reply;
                    last.is_generating = false;
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "conversation poisoned"}),
        }
        self.busy.store(false, Ordering::Relaxed);
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true})
    }

    fn handle_clear(&self) -> serde_json::Value {
        match self.conversation.lock() {
            Ok(mut c) => c.clear(),
            Err(_) => return serde_json::json!({"ok": false, "error": "conversation poisoned"}),
        }
        // Keep `touched = true` so the snapshot surfaces an empty `Some(agent)`
        // (cleared) rather than reverting to `None` (never touched).
        self.busy.store(false, Ordering::Relaxed);
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true})
    }
}

#[cfg(test)]
#[path = "agent_handler_tests.rs"]
mod tests;

//! Intent-to-payload resolution for agent tasks.
//!
//! Extracted from `tasks_handler.rs` to keep that file under the 500-line
//! hard limit (AGENTS.md). The types and free functions here are private to
//! the `tasks_handler` module via `#[path]` inclusion.

use nmp_core::substrate::ActionModule;

use crate::ffi::actions::{
    AgentActionModule, AgentChatAction, AgentTaskIntent, InboxAction, InboxActionModule,
    MemoryAction, MemoryActionModule,
};

pub(super) struct TaskPayload {
    pub action_namespace: String,
    pub action_body: String,
}

pub(super) struct TaskIntentMetadata {
    pub intent_type: String,
    pub intent_label: String,
    pub intent_detail: Option<String>,
}

pub(super) fn task_intent_metadata(intent: Option<&AgentTaskIntent>) -> TaskIntentMetadata {
    match intent {
        Some(AgentTaskIntent::InboxTriage) => TaskIntentMetadata {
            intent_type: "inbox_triage".to_owned(),
            intent_label: "Triage inbox".to_owned(),
            intent_detail: Some("Prioritize new episodes".to_owned()),
        },
        Some(AgentTaskIntent::ClearAgent) => TaskIntentMetadata {
            intent_type: "clear_agent".to_owned(),
            intent_label: "Clear agent chat".to_owned(),
            intent_detail: None,
        },
        Some(AgentTaskIntent::RememberMemory { key, value }) => TaskIntentMetadata {
            intent_type: "remember_memory".to_owned(),
            intent_label: "Remember memory".to_owned(),
            intent_detail: Some(format!("{key} = {value}")),
        },
        Some(AgentTaskIntent::AgentPrompt { prompt }) => TaskIntentMetadata {
            intent_type: "agent_prompt".to_owned(),
            intent_label: "Agent prompt".to_owned(),
            intent_detail: Some(prompt.clone()),
        },
        None => TaskIntentMetadata {
            intent_type: "custom".to_owned(),
            intent_label: "Custom task".to_owned(),
            intent_detail: None,
        },
    }
}

pub(super) fn task_payload_from_intent(intent: &AgentTaskIntent) -> Result<TaskPayload, String> {
    match intent {
        AgentTaskIntent::InboxTriage => task_payload(
            <InboxActionModule as ActionModule>::NAMESPACE.as_str(),
            &InboxAction::Triage,
        ),
        AgentTaskIntent::ClearAgent => task_payload(
            <AgentActionModule as ActionModule>::NAMESPACE.as_str(),
            &AgentChatAction::Clear,
        ),
        AgentTaskIntent::RememberMemory { key, value } => task_payload(
            <MemoryActionModule as ActionModule>::NAMESPACE.as_str(),
            &MemoryAction::Remember {
                key: key.clone(),
                value: value.clone(),
                source: Some("task".into()),
            },
        ),
        AgentTaskIntent::AgentPrompt { prompt } => task_payload(
            <AgentActionModule as ActionModule>::NAMESPACE.as_str(),
            &AgentChatAction::Send {
                message: prompt.clone(),
            },
        ),
    }
}

pub(super) fn task_payload<T: serde::Serialize>(
    action_namespace: &str,
    action: &T,
) -> Result<TaskPayload, String> {
    Ok(TaskPayload {
        action_namespace: action_namespace.to_owned(),
        action_body: serde_json::to_string(action)
            .map_err(|e| format!("failed to encode task intent action: {e}"))?,
    })
}

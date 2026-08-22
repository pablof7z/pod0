use pod0_facade::{AgentMessageRole, AgentModelExecutionRequest};
use serde_json::{Value, json};

pub(super) fn messages(execution: &AgentModelExecutionRequest) -> Vec<Value> {
    execution
        .messages
        .iter()
        .map(|message| {
            let (role, content) = match message.role {
                AgentMessageRole::User => ("user", message.content.clone()),
                AgentMessageRole::Assistant => ("assistant", message.content.clone()),
                AgentMessageRole::Tool => ("user", format!("[tool result]\n{}", message.content)),
                AgentMessageRole::Error => ("system", format!("[error]\n{}", message.content)),
            };
            json!({"role": role, "content": content})
        })
        .collect()
}

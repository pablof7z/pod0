use std::time::Duration;

use pod0_facade::{
    AgentModelExecutionRequest, AgentModelToolCallObservation, AgentModelUsageObservation,
    AgentToolDefinition, AgentToolParameterKind, HostFailureCode, HostObservation,
};
use pod0_live_hosts::{
    CancellationToken, ChatRequest, ChatResponse, ChatTool, HttpLimits, OllamaChatRequest,
    OpenAiChatRequest, ProviderEndpoint, SecretString, ToolChoice,
};
use serde_json::{Value, json};

use crate::protocol::AgentProvider;

use super::agent_payload::to_chat_messages;
use super::{HostExecutor, failed, map_adapter_error};

const CHAT_TIMEOUT: Duration = Duration::from_secs(90);
const CHAT_MAXIMUM_METADATA_BYTES: u64 = 64 * 1024;
const MINIMUM_CHAT_BODY_BYTES: u64 = 64 * 1024;

pub(super) fn execute(
    host: &HostExecutor,
    execution: &AgentModelExecutionRequest,
) -> HostObservation {
    let Some((prefix, model)) = execution.model_reference.split_once(':') else {
        return failed(
            HostFailureCode::ProviderUnavailable,
            "agent model reference has no configured provider",
        );
    };
    let target = match prefix {
        "openai" => AgentProvider::OpenAiCompatible,
        "ollama" => AgentProvider::Ollama,
        _ => {
            return failed(
                HostFailureCode::ProviderUnavailable,
                "agent model provider is unsupported",
            );
        }
    };
    match target {
        AgentProvider::OpenAiCompatible => execute_openai(host, execution, model),
        AgentProvider::Ollama => execute_ollama(host, execution, model),
    }
}

fn chat_request(execution: &AgentModelExecutionRequest, model: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_owned(),
        messages: to_chat_messages(execution),
        tools: execution
            .tool_definitions
            .iter()
            .map(tool_definition_to_chat_tool)
            .collect(),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        maximum_completion_tokens: None,
        timeout: CHAT_TIMEOUT,
        limits: HttpLimits {
            maximum_body_bytes: execution.maximum_output_bytes.max(MINIMUM_CHAT_BODY_BYTES),
            maximum_metadata_bytes: CHAT_MAXIMUM_METADATA_BYTES,
        },
        maximum_output_bytes: execution.maximum_output_bytes,
    }
}

fn tool_definition_to_chat_tool(definition: &AgentToolDefinition) -> ChatTool {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for parameter in &definition.parameters {
        let schema = match &parameter.kind {
            AgentToolParameterKind::Text => json!({"type": "string"}),
            AgentToolParameterKind::Integer { minimum, maximum } => {
                json!({"type": "integer", "minimum": minimum, "maximum": maximum})
            }
            AgentToolParameterKind::DecimalPermille { minimum, maximum } => json!({
                "type": "number",
                "minimum": f64::from(*minimum) / 1_000.0,
                "maximum": f64::from(*maximum) / 1_000.0
            }),
            AgentToolParameterKind::Boolean => json!({"type": "boolean"}),
            AgentToolParameterKind::TextList { maximum_items } => json!({
                "type": "array",
                "items": {"type": "string"},
                "maxItems": maximum_items
            }),
        };
        let mut entry = schema.as_object().cloned().unwrap_or_default();
        entry.insert(
            "description".to_owned(),
            Value::String(parameter.description.clone()),
        );
        properties.insert(parameter.name.clone(), Value::Object(entry));
        if parameter.required {
            required.push(Value::String(parameter.name.clone()));
        }
    }
    ChatTool {
        name: definition.wire_name.clone(),
        description: definition.description.clone(),
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
        strict: None,
    }
}

fn execute_openai(
    host: &HostExecutor,
    execution: &AgentModelExecutionRequest,
    model: &str,
) -> HostObservation {
    let Some(base) = host.config.openai_base_url.as_deref() else {
        return failed(
            HostFailureCode::ProviderUnavailable,
            "OpenAI-compatible endpoint is not configured",
        );
    };
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let endpoint = match host.config.openai_api_key.as_deref() {
        Some(key) => ProviderEndpoint::bearer(url, SecretString::new(key)),
        None => ProviderEndpoint::unauthenticated(url),
    };
    let chat = chat_request(execution, model);
    let result = host.runtime.block_on(host.live.openai_chat(
        OpenAiChatRequest { endpoint, chat },
        &CancellationToken::new(),
    ));
    match result {
        Ok(response) => completed(execution, response),
        Err(error) => map_adapter_error(&error),
    }
}

fn execute_ollama(
    host: &HostExecutor,
    execution: &AgentModelExecutionRequest,
    model: &str,
) -> HostObservation {
    let Some(base) = host.config.ollama_base_url.as_deref() else {
        return failed(
            HostFailureCode::ProviderUnavailable,
            "Ollama endpoint is not configured",
        );
    };
    let url = format!("{}/api/chat", base.trim_end_matches('/'));
    let endpoint = ProviderEndpoint::unauthenticated(url);
    let chat = chat_request(execution, model);
    let result = host.runtime.block_on(host.live.ollama_chat(
        OllamaChatRequest { endpoint, chat },
        &CancellationToken::new(),
    ));
    match result {
        Ok(response) => completed(execution, response),
        Err(error) => map_adapter_error(&error),
    }
}

fn completed(execution: &AgentModelExecutionRequest, response: ChatResponse) -> HostObservation {
    let usage = usage(&response);
    let assistant_text = response.content.unwrap_or_default();
    let proposed_tool_call =
        response
            .tool_calls
            .first()
            .map(|call| AgentModelToolCallObservation {
                provider_call_id: call.id.clone().unwrap_or_default(),
                tool_name: call.name.clone(),
                arguments_json: call.arguments_json.clone(),
            });
    if assistant_text.is_empty() && proposed_tool_call.is_none() {
        return failed(
            HostFailureCode::InvalidResponse,
            "provider returned an empty completion",
        );
    }
    HostObservation::AgentModelCompleted {
        turn_id: execution.turn_id,
        model_fence_id: execution.model_fence_id,
        assistant_text,
        proposed_tool_call,
        usage,
    }
}

fn usage(response: &ChatResponse) -> Option<AgentModelUsageObservation> {
    Some(AgentModelUsageObservation {
        prompt_tokens: response.usage.prompt_tokens?,
        completion_tokens: response.usage.completion_tokens?,
        cached_prompt_tokens: response.usage.cached_prompt_tokens,
    })
}

use pod0_facade::{
    AgentModelExecutionRequest, AgentModelUsageObservation, HostFailureCode, HostObservation,
};
use serde_json::{Value, json};

use crate::protocol::AgentProvider;

use super::agent_payload::messages;
use super::{HostExecutor, failed, network_failure, read_bounded, status_failure};

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
    let body = json!({"model": model, "messages": messages(execution)});
    let mut request = host.client.post(url).json(&body);
    if let Some(key) = host.config.openai_api_key.as_deref() {
        request = request.bearer_auth(key);
    }
    let response = match request.send() {
        Ok(response) => response,
        Err(error) => return network_failure(&error, true),
    };
    if !response.status().is_success() {
        return status_failure(response.status().as_u16(), true);
    }
    let bytes = match read_bounded(response, execution.maximum_output_bytes) {
        Ok(bytes) => bytes,
        Err(observation) => return *observation,
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return failed(
                HostFailureCode::InvalidResponse,
                "provider returned invalid JSON",
            );
        }
    };
    let Some(message) = value.pointer("/choices/0/message") else {
        return failed(
            HostFailureCode::InvalidResponse,
            "provider response contained no message",
        );
    };
    let assistant_text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if contains_tool_call(message) {
        return failed(
            HostFailureCode::Unsupported { wire_code: 1 },
            "provider returned a tool call although this host advertises no tools",
        );
    }
    if assistant_text.is_empty() {
        return failed(
            HostFailureCode::InvalidResponse,
            "provider returned an empty completion",
        );
    }
    let usage = value.get("usage").and_then(openai_usage);
    completed(execution, assistant_text, usage)
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
    let body = json!({"model": model, "stream": false, "messages": messages(execution)});
    let response = match host
        .client
        .post(format!("{}/api/chat", base.trim_end_matches('/')))
        .json(&body)
        .send()
    {
        Ok(response) => response,
        Err(error) => return network_failure(&error, true),
    };
    if !response.status().is_success() {
        return status_failure(response.status().as_u16(), true);
    }
    let bytes = match read_bounded(response, execution.maximum_output_bytes) {
        Ok(bytes) => bytes,
        Err(observation) => return *observation,
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return failed(
                HostFailureCode::InvalidResponse,
                "provider returned invalid JSON",
            );
        }
    };
    let Some(message) = value.get("message") else {
        return failed(
            HostFailureCode::InvalidResponse,
            "provider response contained no message",
        );
    };
    let assistant_text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if contains_tool_call(message) {
        return failed(
            HostFailureCode::Unsupported { wire_code: 1 },
            "provider returned a tool call although this host advertises no tools",
        );
    }
    if assistant_text.is_empty() {
        return failed(
            HostFailureCode::InvalidResponse,
            "provider returned an empty completion",
        );
    }
    let usage = ollama_usage(&value);
    completed(execution, assistant_text, usage)
}

fn completed(
    execution: &AgentModelExecutionRequest,
    assistant_text: String,
    usage: Option<AgentModelUsageObservation>,
) -> HostObservation {
    HostObservation::AgentModelCompleted {
        turn_id: execution.turn_id,
        model_fence_id: execution.model_fence_id,
        assistant_text,
        proposed_tool_call: None,
        usage,
    }
}

fn contains_tool_call(message: &Value) -> bool {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty())
        || message
            .get("function_call")
            .is_some_and(|call| !call.is_null())
}

fn openai_usage(value: &Value) -> Option<AgentModelUsageObservation> {
    Some(AgentModelUsageObservation {
        prompt_tokens: value.get("prompt_tokens")?.as_u64()?,
        completion_tokens: value.get("completion_tokens")?.as_u64()?,
        cached_prompt_tokens: value
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_u64),
    })
}

fn ollama_usage(value: &Value) -> Option<AgentModelUsageObservation> {
    Some(AgentModelUsageObservation {
        prompt_tokens: value.get("prompt_eval_count")?.as_u64()?,
        completion_tokens: value.get("eval_count")?.as_u64()?,
        cached_prompt_tokens: None,
    })
}

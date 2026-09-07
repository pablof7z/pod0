use std::{collections::HashSet, time::Duration};

use serde_json::Value;

use crate::{
    AdapterError, HttpEvidence, HttpLimits, ProviderEndpoint, bounds::enforce_output,
    provider::invalid_provider_response,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistantToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<AssistantToolCall>,
}

impl ChatMessage {
    #[must_use]
    pub fn text(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    #[must_use]
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub strict: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ChatTool>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f64>,
    pub maximum_completion_tokens: Option<u32>,
    pub timeout: Duration,
    pub limits: HttpLimits,
    pub maximum_output_bytes: u64,
}

#[derive(Debug)]
pub struct OpenAiChatRequest {
    pub endpoint: ProviderEndpoint,
    pub chat: ChatRequest,
}

#[derive(Debug)]
pub struct OllamaChatRequest {
    pub endpoint: ProviderEndpoint,
    pub chat: ChatRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments_json: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_prompt_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub refusal: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub response_id: Option<String>,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub created_at: Option<String>,
    pub evidence: HttpEvidence,
}

pub(crate) fn parse_tool_calls(
    value: Option<&Value>,
    openai: bool,
) -> Result<Vec<ToolCall>, AdapterError> {
    let Some(calls) = value else {
        return Ok(Vec::new());
    };
    let calls = calls
        .as_array()
        .ok_or_else(|| invalid_provider_response("chat tool calls"))?;
    let parsed = calls
        .iter()
        .map(|call| {
            if openai && call.get("type").and_then(Value::as_str) != Some("function") {
                return Err(invalid_provider_response("chat tool call type"));
            }
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid_provider_response("chat tool call name"))?
                .to_owned();
            let arguments_value = call
                .pointer("/function/arguments")
                .ok_or_else(|| invalid_provider_response("chat tool call arguments"))?;
            let (arguments_json, arguments) = if let Some(raw) = arguments_value.as_str() {
                let parsed: Value = serde_json::from_str(raw)
                    .map_err(|_| invalid_provider_response("chat tool call arguments"))?;
                (raw.to_owned(), parsed)
            } else if !openai {
                (
                    serde_json::to_string(arguments_value)
                        .map_err(|_| invalid_provider_response("chat tool call arguments"))?,
                    arguments_value.clone(),
                )
            } else {
                return Err(invalid_provider_response("chat tool call arguments"));
            };
            if !arguments.is_object() {
                return Err(invalid_provider_response("chat tool call arguments"));
            }
            let id = call.get("id").and_then(Value::as_str).map(str::to_owned);
            if openai && id.is_none() {
                return Err(invalid_provider_response("chat tool call id"));
            }
            Ok(ToolCall {
                id,
                name,
                arguments_json,
                arguments,
            })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    let mut ids = HashSet::new();
    if parsed
        .iter()
        .filter_map(|call| call.id.as_deref())
        .any(|id| !ids.insert(id))
    {
        return Err(invalid_provider_response("duplicate chat tool call id"));
    }
    Ok(parsed)
}

pub(crate) fn enforce_chat_output(
    content: Option<&str>,
    refusal: Option<&str>,
    calls: &[ToolCall],
    limit: u64,
) -> Result<(), AdapterError> {
    let size = content.map_or(0, str::len)
        + refusal.map_or(0, str::len)
        + calls
            .iter()
            .map(|call| {
                call.name.len() + call.arguments_json.len() + call.id.as_deref().map_or(0, str::len)
            })
            .sum::<usize>();
    enforce_output(size, limit)
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;

use serde_json::Value;

use crate::{
    AdapterError, CancellationToken, ChatResponse, LiveHosts, OpenAiChatRequest, ProviderKind,
    TokenUsage,
    bounds::bounded_body,
    chat::{enforce_chat_output, parse_tool_calls},
    chat_request::request_body,
    provider::invalid_provider_response,
};

impl LiveHosts {
    pub async fn openai_chat(
        &self,
        request: OpenAiChatRequest,
        cancellation: &CancellationToken,
    ) -> Result<ChatResponse, AdapterError> {
        cancellation.check()?;
        request.chat.limits.validate()?;
        let body = request_body(&request.chat, false)?;
        self.run(request.chat.timeout, cancellation, async {
            let response = self
                .provider_request(&request.endpoint, ProviderKind::OpenAiCompatible)?
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(
                    response,
                    ProviderKind::OpenAiCompatible,
                    request.chat.limits,
                )
                .await?;
            let bytes = bounded_body(response, request.chat.limits.maximum_body_bytes).await?;
            parse_openai(&bytes, evidence, request.chat.maximum_output_bytes)
        })
        .await
    }
}

fn parse_openai(
    bytes: &[u8],
    evidence: crate::HttpEvidence,
    maximum_output_bytes: u64,
) -> Result<ChatResponse, AdapterError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| invalid_provider_response("OpenAI chat JSON"))?;
    let message = value
        .pointer("/choices/0/message")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_provider_response("OpenAI chat message"))?;
    let content = match message.get("content") {
        None | Some(Value::Null) => None,
        Some(Value::String(content)) => Some(content.clone()),
        _ => return Err(invalid_provider_response("OpenAI chat content")),
    };
    let refusal = match message.get("refusal") {
        None | Some(Value::Null) => None,
        Some(Value::String(refusal)) if !refusal.is_empty() => Some(refusal.clone()),
        _ => return Err(invalid_provider_response("OpenAI chat refusal")),
    };
    let tool_calls = parse_tool_calls(message.get("tool_calls"), true)?;
    if content.is_none() && refusal.is_none() && tool_calls.is_empty() {
        return Err(invalid_provider_response("OpenAI chat output"));
    }
    enforce_chat_output(
        content.as_deref(),
        refusal.as_deref(),
        &tool_calls,
        maximum_output_bytes,
    )?;
    Ok(ChatResponse {
        content,
        refusal,
        tool_calls,
        usage: openai_usage(value.get("usage"))?,
        response_id: optional_string(&value, "id")?,
        model: optional_string(&value, "model")?,
        finish_reason: optional_string(
            value
                .pointer("/choices/0")
                .ok_or_else(|| invalid_provider_response("OpenAI chat choice"))?,
            "finish_reason",
        )?,
        created_at: optional_u64(value.get("created"))?.map(|value| value.to_string()),
        evidence,
    })
}

fn openai_usage(value: Option<&Value>) -> Result<TokenUsage, AdapterError> {
    let Some(value) = value else {
        return Ok(TokenUsage::default());
    };
    let object = value
        .as_object()
        .ok_or_else(|| invalid_provider_response("OpenAI token usage"))?;
    Ok(TokenUsage {
        prompt_tokens: optional_u64(object.get("prompt_tokens"))?,
        completion_tokens: optional_u64(object.get("completion_tokens"))?,
        total_tokens: optional_u64(object.get("total_tokens"))?,
        cached_prompt_tokens: optional_u64(value.pointer("/prompt_tokens_details/cached_tokens"))?,
        reasoning_tokens: optional_u64(
            value.pointer("/completion_tokens_details/reasoning_tokens"),
        )?,
    })
}

fn optional_u64(value: Option<&Value>) -> Result<Option<u64>, AdapterError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid_provider_response("token usage")),
    }
}

fn optional_string(value: &Value, key: &'static str) -> Result<Option<String>, AdapterError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_provider_response(key)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn evidence() -> crate::HttpEvidence {
        crate::HttpEvidence {
            status: 200,
            final_url: "https://example.invalid/v1/chat/completions".to_owned(),
            redirects: Vec::new(),
            entity_tag: None,
            last_modified: None,
            content_type: Some("application/json".to_owned()),
            content_length: None,
        }
    }

    #[test]
    fn parses_all_tool_calls_and_usage() {
        let value = json!({
            "id": "response-1",
            "model": "model-1",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {"content": null, "tool_calls": [
                    {"id":"one","type":"function","function":{"name":"first","arguments":"{\"x\":1}"}},
                    {"id":"two","type":"function","function":{"name":"second","arguments":"{\"y\":2}"}}
                ]}
            }],
            "usage": {
                "prompt_tokens": 7,
                "completion_tokens": 3,
                "total_tokens": 10,
                "prompt_tokens_details": {"cached_tokens": 2}
            }
        });
        let response =
            parse_openai(&serde_json::to_vec(&value).unwrap(), evidence(), 1_024).unwrap();
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.usage.cached_prompt_tokens, Some(2));
    }

    #[test]
    fn output_bound_includes_tool_arguments() {
        let value = json!({
            "choices": [{"message": {
                "content": null,
                "tool_calls": [{"id":"one","type":"function","function":{"name":"f","arguments":"{\"long\":\"value\"}"}}]
            }}]
        });
        assert!(matches!(
            parse_openai(&serde_json::to_vec(&value).unwrap(), evidence(), 2),
            Err(AdapterError::Size(_))
        ));
    }

    #[test]
    fn refusal_is_preserved_separately_and_bounded() {
        let value = json!({
            "choices": [{"message": {"content": null, "refusal": "cannot comply"}}]
        });
        let response = parse_openai(&serde_json::to_vec(&value).unwrap(), evidence(), 20).unwrap();
        assert_eq!(response.content, None);
        assert_eq!(response.refusal.as_deref(), Some("cannot comply"));

        assert!(matches!(
            parse_openai(&serde_json::to_vec(&value).unwrap(), evidence(), 3),
            Err(AdapterError::Size(_))
        ));
    }
}

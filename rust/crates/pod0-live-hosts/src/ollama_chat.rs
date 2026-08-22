use serde_json::Value;

use crate::{
    AdapterError, CancellationToken, ChatResponse, LiveHosts, OllamaChatRequest, ProviderKind,
    TokenUsage,
    bounds::bounded_body,
    chat::{enforce_chat_output, parse_tool_calls},
    chat_request::request_body,
    provider::invalid_provider_response,
};

impl LiveHosts {
    pub async fn ollama_chat(
        &self,
        request: OllamaChatRequest,
        cancellation: &CancellationToken,
    ) -> Result<ChatResponse, AdapterError> {
        cancellation.check()?;
        request.chat.limits.validate()?;
        let body = request_body(&request.chat, true)?;
        self.run(request.chat.timeout, cancellation, async {
            let response = self
                .provider_request(&request.endpoint, ProviderKind::Ollama)?
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(response, ProviderKind::Ollama, request.chat.limits)
                .await?;
            let bytes = bounded_body(response, request.chat.limits.maximum_body_bytes).await?;
            parse_ollama(&bytes, evidence, request.chat.maximum_output_bytes)
        })
        .await
    }
}

fn parse_ollama(
    bytes: &[u8],
    evidence: crate::HttpEvidence,
    maximum_output_bytes: u64,
) -> Result<ChatResponse, AdapterError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| invalid_provider_response("Ollama chat JSON"))?;
    if value.get("done").and_then(Value::as_bool) != Some(true) {
        return Err(invalid_provider_response("Ollama completion state"));
    }
    let message = value
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_provider_response("Ollama chat message"))?;
    let content = match message.get("content") {
        None | Some(Value::Null) => None,
        Some(Value::String(content)) => Some(content.clone()),
        _ => return Err(invalid_provider_response("Ollama chat content")),
    };
    let tool_calls = parse_tool_calls(message.get("tool_calls"), false)?;
    if content.is_none() && tool_calls.is_empty() {
        return Err(invalid_provider_response("Ollama chat output"));
    }
    enforce_chat_output(content.as_deref(), None, &tool_calls, maximum_output_bytes)?;
    Ok(ChatResponse {
        content,
        refusal: None,
        tool_calls,
        usage: TokenUsage {
            prompt_tokens: optional_u64(&value, "prompt_eval_count")?,
            completion_tokens: optional_u64(&value, "eval_count")?,
            total_tokens: None,
            cached_prompt_tokens: None,
            reasoning_tokens: None,
        },
        response_id: None,
        model: optional_string(&value, "model")?,
        finish_reason: optional_string(&value, "done_reason")?,
        created_at: optional_string(&value, "created_at")?,
        evidence,
    })
}

fn optional_u64(value: &Value, key: &'static str) -> Result<Option<u64>, AdapterError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid_provider_response(key)),
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

    #[test]
    fn incomplete_non_streaming_response_is_rejected() {
        let value = json!({"done": false, "message": {"content": "partial"}});
        let evidence = crate::HttpEvidence {
            status: 200,
            final_url: "http://localhost/api/chat".to_owned(),
            redirects: Vec::new(),
            entity_tag: None,
            last_modified: None,
            content_type: None,
            content_length: None,
        };
        assert!(parse_ollama(&serde_json::to_vec(&value).unwrap(), evidence, 100).is_err());
    }
}

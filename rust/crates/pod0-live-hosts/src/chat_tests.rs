use serde_json::json;

use super::*;
use crate::chat_request::request_body;

#[test]
fn openai_tool_arguments_must_be_valid_json_objects() {
    let value =
        json!([{"id":"call-1","type":"function","function":{"name":"search","arguments":"[]"}}]);
    assert!(parse_tool_calls(Some(&value), true).is_err());
}

#[test]
fn ollama_object_arguments_are_preserved() {
    let value = json!([{"function":{"name":"search","arguments":{"query":"rust"}}}]);
    let calls = parse_tool_calls(Some(&value), false).unwrap();
    assert_eq!(calls[0].arguments["query"], "rust");
    assert_eq!(calls[0].id, None);
}

#[test]
fn ollama_request_uses_provider_native_tool_arguments_and_options() {
    let chat = ChatRequest {
        model: "model".to_owned(),
        messages: vec![ChatMessage {
            role: ChatRole::Assistant,
            content: None,
            tool_call_id: None,
            tool_calls: vec![AssistantToolCall {
                id: None,
                name: "search".to_owned(),
                arguments: json!({"query":"rust"}),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        temperature: Some(0.2),
        maximum_completion_tokens: Some(12),
        timeout: std::time::Duration::from_secs(1),
        limits: HttpLimits {
            maximum_body_bytes: 1,
            maximum_metadata_bytes: 1,
        },
        maximum_output_bytes: 1,
    };
    let body = request_body(&chat, true).unwrap();
    assert_eq!(
        body.pointer("/messages/0/tool_calls/0/function/arguments/query"),
        Some(&json!("rust"))
    );
    assert_eq!(body.pointer("/options/num_predict"), Some(&json!(12)));
}

#[test]
fn openai_request_preserves_json_schema_and_strict_mode() {
    let mut chat = chat_request();
    chat.tools.push(ChatTool {
        name: "search".to_owned(),
        description: "Search live data".to_owned(),
        parameters: json!({
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
            "additionalProperties": false
        }),
        strict: Some(true),
    });
    let body = request_body(&chat, false).unwrap();
    assert_eq!(body.pointer("/tools/0/function/strict"), Some(&json!(true)));
    assert_eq!(
        body.pointer("/tools/0/function/parameters/required/0"),
        Some(&json!("query"))
    );
}

#[test]
fn ollama_none_omits_tools_and_required_is_rejected() {
    let mut chat = chat_request();
    chat.tools.push(tool());
    chat.tool_choice = ToolChoice::None;
    let body = request_body(&chat, true).unwrap();
    assert!(body.get("tools").is_none());

    chat.tool_choice = ToolChoice::Required;
    assert!(request_body(&chat, true).is_err());
}

#[test]
fn openai_required_is_sent_and_requires_declared_tools() {
    let mut chat = chat_request();
    chat.tool_choice = ToolChoice::Required;
    assert!(request_body(&chat, false).is_err());

    chat.tools.push(tool());
    let body = request_body(&chat, false).unwrap();
    assert_eq!(body.get("tool_choice"), Some(&json!("required")));
    assert!(body.get("tools").is_some());
}

fn tool() -> ChatTool {
    ChatTool {
        name: "search".to_owned(),
        description: "Search".to_owned(),
        parameters: json!({"type": "object"}),
        strict: None,
    }
}

fn chat_request() -> ChatRequest {
    ChatRequest {
        model: "model".to_owned(),
        messages: vec![ChatMessage::text(ChatRole::User, "hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        maximum_completion_tokens: None,
        timeout: std::time::Duration::from_secs(1),
        limits: HttpLimits {
            maximum_body_bytes: 1,
            maximum_metadata_bytes: 1,
        },
        maximum_output_bytes: 1,
    }
}

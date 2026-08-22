use serde_json::{Map, Value, json};

use crate::{
    AdapterError, AssistantToolCall, ChatMessage, ChatRequest, ChatRole, ChatTool, ToolChoice,
    provider::invalid_provider_response,
};

pub(crate) fn request_body(chat: &ChatRequest, ollama: bool) -> Result<Value, AdapterError> {
    if chat.model.trim().is_empty() || chat.messages.is_empty() || chat.maximum_output_bytes == 0 {
        return Err(invalid_provider_response("chat request"));
    }
    let messages = chat
        .messages
        .iter()
        .map(|message| message_value(message, ollama))
        .collect::<Result<Vec<_>, _>>()?;
    let include_tools = match (ollama, chat.tool_choice) {
        (true, ToolChoice::Required) => {
            return Err(invalid_provider_response("Ollama required tool choice"));
        }
        (true, ToolChoice::None) => false,
        (false, ToolChoice::Required) if chat.tools.is_empty() => {
            return Err(invalid_provider_response(
                "OpenAI required tool choice without tools",
            ));
        }
        _ => true,
    };
    let mut body = Map::from_iter([
        ("model".to_owned(), Value::String(chat.model.clone())),
        ("messages".to_owned(), Value::Array(messages)),
        ("stream".to_owned(), Value::Bool(false)),
    ]);
    if include_tools && !chat.tools.is_empty() {
        let tools = chat
            .tools
            .iter()
            .map(|tool| tool_value(tool, ollama))
            .collect::<Vec<_>>();
        body.insert("tools".to_owned(), Value::Array(tools));
        if !ollama {
            body.insert(
                "tool_choice".to_owned(),
                Value::String(
                    match chat.tool_choice {
                        ToolChoice::Auto => "auto",
                        ToolChoice::None => "none",
                        ToolChoice::Required => "required",
                    }
                    .to_owned(),
                ),
            );
        }
    }
    insert_options(&mut body, chat, ollama)?;
    Ok(Value::Object(body))
}

fn insert_options(
    body: &mut Map<String, Value>,
    chat: &ChatRequest,
    ollama: bool,
) -> Result<(), AdapterError> {
    if let Some(temperature) = chat.temperature {
        if !temperature.is_finite() {
            return Err(invalid_provider_response("chat temperature"));
        }
        if ollama {
            body.insert("options".to_owned(), json!({"temperature": temperature}));
        } else {
            body.insert("temperature".to_owned(), json!(temperature));
        }
    }
    if let Some(maximum) = chat.maximum_completion_tokens {
        if ollama {
            let options = body
                .entry("options")
                .or_insert_with(|| Value::Object(Map::new()));
            options["num_predict"] = json!(maximum);
        } else {
            body.insert("max_completion_tokens".to_owned(), json!(maximum));
        }
    }
    Ok(())
}

fn message_value(message: &ChatMessage, ollama: bool) -> Result<Value, AdapterError> {
    let role = match message.role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    };
    if message.role == ChatRole::Tool && message.tool_call_id.is_none() {
        return Err(invalid_provider_response("tool result message"));
    }
    let mut value = Map::from_iter([("role".to_owned(), Value::String(role.to_owned()))]);
    value.insert(
        "content".to_owned(),
        message.content.clone().map_or(Value::Null, Value::String),
    );
    if let Some(id) = &message.tool_call_id {
        value.insert("tool_call_id".to_owned(), Value::String(id.clone()));
    }
    if !message.tool_calls.is_empty() {
        value.insert(
            "tool_calls".to_owned(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| assistant_tool_call(call, ollama))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
    }
    Ok(Value::Object(value))
}

fn assistant_tool_call(call: &AssistantToolCall, ollama: bool) -> Result<Value, AdapterError> {
    let arguments = if ollama {
        call.arguments.clone()
    } else {
        Value::String(
            serde_json::to_string(&call.arguments)
                .map_err(|_| invalid_provider_response("assistant tool call"))?,
        )
    };
    let mut value = json!({
        "type": "function",
        "function": {"name": call.name, "arguments": arguments}
    });
    if let Some(id) = &call.id {
        value["id"] = Value::String(id.clone());
    }
    Ok(value)
}

fn tool_value(tool: &ChatTool, ollama: bool) -> Value {
    let mut function = Map::from_iter([
        ("name".to_owned(), Value::String(tool.name.clone())),
        (
            "description".to_owned(),
            Value::String(tool.description.clone()),
        ),
        ("parameters".to_owned(), tool.parameters.clone()),
    ]);
    if !ollama && let Some(strict) = tool.strict {
        function.insert("strict".to_owned(), Value::Bool(strict));
    }
    json!({"type": "function", "function": function})
}

//! Rust-owned conversation-history agent tool policy.
//!
//! Swift supplies raw in-app and Nostr conversation facts. Rust owns tool arg
//! normalization, limit caps, source ordering, lexical search, row shape,
//! snippet truncation, and Nostr display fallbacks.

use std::ffi::{c_char, CStr, CString};

use nostr::nips::nip19::ToBech32;
use serde::Deserialize;
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const LIST_DEFAULT_LIMIT: usize = 20;
const LIST_MAX_LIMIT: usize = 50;
const SEARCH_DEFAULT_LIMIT: usize = 10;
const SEARCH_MAX_LIMIT: usize = 25;
const TITLE_CHARS: usize = 80;
const FIRST_MESSAGE_CHARS: usize = 200;
const HIT_SNIPPET_CHARS: usize = 400;

#[derive(Debug, Deserialize)]
struct ConversationHistoryRequest {
    op: String,
    #[serde(default)]
    args: Value,
    #[serde(default)]
    in_app: Vec<InAppConversation>,
    #[serde(default)]
    nostr: Vec<NostrConversation>,
    #[serde(default)]
    friends: Vec<FriendRow>,
}

#[derive(Debug, Deserialize)]
struct InAppConversation {
    id: String,
    title: String,
    updated_at: i64,
    #[serde(default)]
    messages: Vec<InAppMessage>,
}

#[derive(Debug, Deserialize)]
struct InAppMessage {
    role: String,
    text: String,
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct NostrConversation {
    root_event_id: String,
    counterparty_pubkey: String,
    first_seen: i64,
    last_touched: i64,
    #[serde(default)]
    turns: Vec<NostrTurn>,
}

#[derive(Debug, Deserialize)]
struct NostrTurn {
    direction: String,
    content: String,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct FriendRow {
    identifier: String,
    display_name: String,
}

fn encode_json(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_conversation_history(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_conversation_history",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: ConversationHistoryRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_json(tool_error("Invalid conversation-history request")),
            };
            encode_json(dispatch(request))
        },
    )
}

fn dispatch(mut request: ConversationHistoryRequest) -> Value {
    match request.op.as_str() {
        "list_conversations" => list_conversations(&mut request),
        "search_conversations" => search_conversations(&mut request),
        _ => tool_error("Unknown conversation-history operation"),
    }
}

fn list_conversations(request: &mut ConversationHistoryRequest) -> Value {
    let source = source_arg(&request.args);
    let limit = limit_arg(&request.args, LIST_DEFAULT_LIMIT, LIST_MAX_LIMIT);
    let friends = friend_map(&request.friends);
    let mut results = Vec::new();

    if source == "in_app" || source == "all" {
        request.in_app.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        for conversation in request.in_app.iter().take(limit) {
            results.push(serialize_in_app_summary(conversation));
        }
    }

    if source == "nostr" || source == "all" {
        request
            .nostr
            .sort_by(|a, b| b.last_touched.cmp(&a.last_touched));
        let remaining = limit.saturating_sub(results.len());
        for conversation in request.nostr.iter().take(remaining) {
            results.push(serialize_nostr_summary(conversation, &friends));
        }
    }
    let count = results.len();

    json!({
        "success": true,
        "conversations": results,
        "count": count,
        "source": source,
    })
}

fn search_conversations(request: &mut ConversationHistoryRequest) -> Value {
    let query = string_arg(&request.args, "query").trim().to_string();
    if query.is_empty() {
        return tool_error("Missing or empty 'query'");
    }
    let source = source_arg(&request.args);
    let limit = limit_arg(&request.args, SEARCH_DEFAULT_LIMIT, SEARCH_MAX_LIMIT);
    let needle = query.to_lowercase();
    let friends = friend_map(&request.friends);
    let mut hits = Vec::new();

    if source == "in_app" || source == "all" {
        request.in_app.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        'in_app: for conversation in &request.in_app {
            for message in &conversation.messages {
                if !message.text.to_lowercase().contains(&needle) {
                    continue;
                }
                hits.push(serialize_in_app_hit(message, conversation));
                if hits.len() >= limit {
                    break 'in_app;
                }
            }
        }
    }

    if (source == "nostr" || source == "all") && hits.len() < limit {
        request
            .nostr
            .sort_by(|a, b| b.last_touched.cmp(&a.last_touched));
        'nostr: for conversation in &request.nostr {
            for turn in &conversation.turns {
                if !turn.content.to_lowercase().contains(&needle) {
                    continue;
                }
                hits.push(serialize_nostr_hit(turn, conversation, &friends));
                if hits.len() >= limit {
                    break 'nostr;
                }
            }
        }
    }

    json!({
        "success": true,
        "query": query,
        "total_found": hits.len(),
        "results": hits,
        "source": source,
    })
}

fn serialize_in_app_summary(conversation: &InAppConversation) -> Value {
    let user_count = conversation.messages.iter().filter(|m| m.role == "user").count();
    let assistant_count = conversation
        .messages
        .iter()
        .filter(|m| m.role == "assistant")
        .count();
    let title = non_empty(&conversation.title)
        .unwrap_or_else(|| truncate(&first_user_message(conversation), TITLE_CHARS));
    let first = first_user_message(conversation);
    let mut row = json!({
        "source": "in_app",
        "conversation_id": conversation.id,
        "updated_at": conversation.updated_at,
        "message_count": conversation.messages.len(),
        "user_message_count": user_count,
        "assistant_message_count": assistant_count,
        "title": title,
    });
    if !first.is_empty() {
        row["first_user_message"] = json!(truncate(&first, FIRST_MESSAGE_CHARS));
    }
    row
}

fn serialize_nostr_summary(
    conversation: &NostrConversation,
    friends: &std::collections::HashMap<String, String>,
) -> Value {
    let first = conversation
        .turns
        .first()
        .map(|turn| truncate(&turn.content, FIRST_MESSAGE_CHARS));
    let mut row = json!({
        "source": "nostr",
        "root_event_id": conversation.root_event_id,
        "counterparty": display_name(&conversation.counterparty_pubkey, friends),
        "counterparty_pubkey": conversation.counterparty_pubkey,
        "first_seen": conversation.first_seen,
        "last_touched": conversation.last_touched,
        "turn_count": conversation.turns.len(),
    });
    if let Some(first) = first {
        row["first_message"] = json!(first);
    }
    row
}

fn serialize_in_app_hit(message: &InAppMessage, conversation: &InAppConversation) -> Value {
    let title = non_empty(&conversation.title)
        .unwrap_or_else(|| truncate(&first_user_message(conversation), TITLE_CHARS));
    json!({
        "source": "in_app",
        "conversation_id": conversation.id,
        "conversation_title": title,
        "conversation_updated_at": conversation.updated_at,
        "role": normalized_role(&message.role),
        "timestamp": message.timestamp,
        "snippet": truncate(&message.text, HIT_SNIPPET_CHARS),
    })
}

fn serialize_nostr_hit(
    turn: &NostrTurn,
    conversation: &NostrConversation,
    friends: &std::collections::HashMap<String, String>,
) -> Value {
    json!({
        "source": "nostr",
        "root_event_id": conversation.root_event_id,
        "counterparty": display_name(&conversation.counterparty_pubkey, friends),
        "direction": turn.direction,
        "timestamp": turn.created_at,
        "snippet": truncate(&turn.content, HIT_SNIPPET_CHARS),
    })
}

fn source_arg(args: &Value) -> String {
    match string_arg(args, "source").trim().to_lowercase().as_str() {
        "in_app" => "in_app".into(),
        "nostr" => "nostr".into(),
        _ => "all".into(),
    }
}

fn limit_arg(args: &Value, default_value: usize, max: usize) -> usize {
    let raw = args.get("limit");
    let parsed = match raw {
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize),
        Some(Value::String(s)) => s.trim().parse::<usize>().ok(),
        _ => None,
    }
    .unwrap_or(default_value);
    parsed.clamp(1, max)
}

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn first_user_message(conversation: &InAppConversation) -> String {
    conversation
        .messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| message.text.clone())
        .unwrap_or_default()
}

fn normalized_role(role: &str) -> &'static str {
    match role {
        "user" => "user",
        "assistant" => "assistant",
        _ => "other",
    }
}

fn friend_map(friends: &[FriendRow]) -> std::collections::HashMap<String, String> {
    friends
        .iter()
        .filter_map(|friend| {
            let id = friend.identifier.trim();
            let name = friend.display_name.trim();
            if id.is_empty() || name.is_empty() {
                None
            } else {
                Some((id.to_string(), name.to_string()))
            }
        })
        .collect()
}

fn display_name(pubkey: &str, friends: &std::collections::HashMap<String, String>) -> String {
    friends
        .get(pubkey)
        .cloned()
        .unwrap_or_else(|| short_npub(pubkey))
}

fn short_npub(hex: &str) -> String {
    let full = nostr::PublicKey::parse(hex)
        .and_then(|pk| pk.to_bech32())
        .unwrap_or_else(|_| hex.to_string());
    if full.starts_with("npub1") && full.chars().count() > 17 {
        let head = full.chars().take(12).collect::<String>();
        let tail = full
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        format!("{head}...{tail}")
    } else {
        full
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value.chars().take(limit).collect()
    }
}

fn tool_error(message: &str) -> Value {
    json!({ "error": message })
}

//! Data-manipulation projections for agent action tools.
//!
//! Covers chat-history normalisation, agent activity and run ledgers,
//! pending-friend tracking, and category summary projection — all of which
//! operate on arrays / collections rather than producing simple result
//! envelopes.

use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use super::{
    bool_arg, bool_arg_default, bounded_usize_arg, insert_optional, optional_string_arg,
    string_arg, string_array,
};

// ---------------------------------------------------------------------------
// Chat history
// ---------------------------------------------------------------------------

pub(super) fn chat_history_upsert(request: &Value) -> Value {
    let mut conversations = request
        .get("conversations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(mut conversation) = request.get("conversation").cloned() else {
        return chat_history_normalize_value(conversations);
    };
    let id = string_arg(&conversation, "id");
    let message_count = conversation
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    if message_count == 0 {
        conversations.retain(|existing| string_arg(existing, "id") != id);
    } else {
        cap_conversation_messages(&mut conversation);
        if let Some(index) = conversations.iter().position(|existing| string_arg(existing, "id") == id) {
            conversations[index] = conversation;
        } else {
            conversations.push(conversation);
        }
    }
    chat_history_normalize_value(conversations)
}

pub(super) fn chat_history_normalize(request: &Value) -> Value {
    let conversations = request
        .get("conversations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    chat_history_normalize_value(conversations)
}

pub(super) fn chat_history_wrap_legacy(request: &Value) -> Value {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if messages.is_empty() {
        return json!({"conversations": []});
    }
    let updated_at = messages
        .last()
        .map(|message| string_arg(message, "timestamp"))
        .filter(|timestamp| !timestamp.is_empty())
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    let created_at = messages
        .first()
        .map(|message| string_arg(message, "timestamp"))
        .filter(|timestamp| !timestamp.is_empty())
        .unwrap_or_else(|| updated_at.clone());
    let conversation = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "title": "",
        "messages": cap_messages(messages),
        "isUpgraded": bool_arg(request, "is_upgraded"),
        "enabledSkills": [],
        "isScheduledTask": false,
        "createdAt": created_at,
        "updatedAt": updated_at,
    });
    json!({"conversations": [conversation]})
}

fn chat_history_normalize_value(mut conversations: Vec<Value>) -> Value {
    for conversation in &mut conversations {
        cap_conversation_messages(conversation);
    }
    conversations.retain(|conversation| {
        conversation
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| !messages.is_empty())
            .unwrap_or(false)
    });
    conversations.sort_by(|lhs, rhs| string_arg(rhs, "updatedAt").cmp(&string_arg(lhs, "updatedAt")));
    conversations.truncate(50);
    json!({"conversations": conversations})
}

fn cap_conversation_messages(conversation: &mut Value) {
    if let Some(messages) = conversation.get_mut("messages").and_then(Value::as_array_mut) {
        if messages.len() > 100 {
            let start = messages.len() - 100;
            *messages = messages.split_off(start);
        }
    }
}

fn cap_messages(mut messages: Vec<Value>) -> Vec<Value> {
    if messages.len() > 100 {
        let start = messages.len() - 100;
        messages.split_off(start)
    } else {
        messages
    }
}

// ---------------------------------------------------------------------------
// Agent activity ledger
// ---------------------------------------------------------------------------

pub(super) fn agent_activity_record(request: &Value) -> Value {
    let mut entries = activity_entries(request);
    if let Some(entry) = request.get("entry").cloned() {
        entries.push(entry);
    }
    json!({"entries": trim_agent_activity(entries)})
}

pub(super) fn agent_activity_prune(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let entries: Vec<Value> = activity_entries(request)
        .into_iter()
        .filter(|entry| {
            cutoff.is_empty() || string_arg(entry, "timestamp").as_str() >= cutoff.as_str()
        })
        .collect();
    json!({"entries": entries})
}

pub(super) fn agent_activity_for_batch(request: &Value) -> Value {
    let batch_id = string_arg(request, "batch_id");
    let mut entries: Vec<Value> = activity_entries(request)
        .into_iter()
        .filter(|entry| string_arg(entry, "batchID") == batch_id)
        .collect();
    sort_agent_activity_newest_first(&mut entries);
    json!({"entries": entries})
}

pub(super) fn agent_activity_sorted(request: &Value) -> Value {
    let mut entries = activity_entries(request);
    sort_agent_activity_newest_first(&mut entries);
    json!({"entries": entries})
}

pub(super) fn agent_activity_active_count(request: &Value) -> Value {
    let count = activity_entries(request)
        .iter()
        .filter(|entry| !bool_arg(entry, "undone"))
        .count();
    json!({"count": count})
}

pub(super) fn agent_activity_undo_batch_ids(request: &Value) -> Value {
    let batch_id = string_arg(request, "batch_id");
    let ids: Vec<String> = activity_entries(request)
        .iter()
        .filter(|entry| string_arg(entry, "batchID") == batch_id && !bool_arg(entry, "undone"))
        .map(|entry| string_arg(entry, "id"))
        .filter(|id| !id.is_empty())
        .collect();
    json!({"ids": ids})
}

pub(super) fn agent_activity_mark_undone(request: &Value) -> Value {
    let entry_id = string_arg(request, "entry_id");
    let mut entries = activity_entries(request);
    for entry in &mut entries {
        if string_arg(entry, "id") == entry_id {
            entry["undone"] = json!(true);
            break;
        }
    }
    json!({"entries": entries})
}

fn activity_entries(request: &Value) -> Vec<Value> {
    request
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn trim_agent_activity(entries: Vec<Value>) -> Vec<Value> {
    const MAX_ACTIVITY_ENTRIES: usize = 200;
    if entries.len() <= MAX_ACTIVITY_ENTRIES {
        return entries;
    }
    let excess = entries.len() - MAX_ACTIVITY_ENTRIES;
    let mut removed = 0usize;
    let mut remove = vec![false; entries.len()];
    for (idx, entry) in entries.iter().enumerate() {
        if removed >= excess {
            break;
        }
        if bool_arg(entry, "undone") {
            remove[idx] = true;
            removed += 1;
        }
    }
    for idx in 0..entries.len() {
        if removed >= excess {
            break;
        }
        if !remove[idx] {
            remove[idx] = true;
            removed += 1;
        }
    }
    entries
        .into_iter()
        .enumerate()
        .filter_map(|(idx, entry)| (!remove[idx]).then_some(entry))
        .collect()
}

fn sort_agent_activity_newest_first(entries: &mut [Value]) {
    entries.sort_by(|lhs, rhs| string_arg(rhs, "timestamp").cmp(&string_arg(lhs, "timestamp")));
}

// ---------------------------------------------------------------------------
// Agent run ledger
// ---------------------------------------------------------------------------

pub(super) fn agent_run_record(request: &Value) -> Value {
    let mut runs = Vec::new();
    if let Some(run) = request.get("run").cloned() {
        runs.push(run);
    }
    runs.extend(agent_runs(request));
    json!({"runs": cap_agent_runs(runs)})
}

pub(super) fn agent_run_normalize(request: &Value) -> Value {
    json!({"runs": cap_agent_runs(agent_runs(request))})
}

pub(super) fn agent_run_filter(request: &Value) -> Value {
    let sources: std::collections::HashSet<String> = string_array(request, "sources")
        .into_iter()
        .collect();
    let outcomes: std::collections::HashSet<String> = string_array(request, "outcomes")
        .into_iter()
        .collect();
    let tool_query = string_arg(request, "tool_name_query").to_ascii_lowercase();
    let runs: Vec<Value> = agent_runs(request)
        .into_iter()
        .filter(|run| {
            (sources.is_empty() || sources.contains(&string_arg(run, "source")))
                && (outcomes.is_empty() || outcomes.contains(&string_arg(run, "finalOutcome")))
                && (tool_query.is_empty() || run_has_tool_query(run, &tool_query))
        })
        .collect();
    json!({"runs": runs})
}

fn agent_runs(request: &Value) -> Vec<Value> {
    request
        .get("runs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn cap_agent_runs(mut runs: Vec<Value>) -> Vec<Value> {
    const MAX_RETAINED_RUNS: usize = 500;
    runs.truncate(MAX_RETAINED_RUNS);
    runs
}

fn run_has_tool_query(run: &Value, query: &str) -> bool {
    run.get("turns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|turn| {
            turn.get("toolDispatches")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .any(|dispatch| {
            string_arg(dispatch, "toolName")
                .to_ascii_lowercase()
                .contains(query)
        })
}

// ---------------------------------------------------------------------------
// Pending friend messages
// ---------------------------------------------------------------------------

pub(super) fn pending_friend_register(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let mut messages = pending_friend_messages(request, &cutoff);
    if let Some(message) = request.get("message").cloned() {
        let sent_event_id = string_arg(&message, "sentEventID");
        messages.retain(|existing| string_arg(existing, "sentEventID") != sent_event_id);
        messages.push(message);
    }
    json!({"messages": messages})
}

pub(super) fn pending_friend_claim(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let root_event_id = string_arg(request, "root_event_id");
    let mut claimed: Option<Value> = None;
    let messages: Vec<Value> = pending_friend_messages(request, &cutoff)
        .into_iter()
        .filter_map(|message| {
            if claimed.is_none() && string_arg(&message, "sentEventID") == root_event_id {
                claimed = Some(message);
                None
            } else {
                Some(message)
            }
        })
        .collect();
    json!({"messages": messages, "claimed": claimed})
}

pub(super) fn pending_friend_has(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let root_event_id = string_arg(request, "root_event_id");
    let messages = pending_friend_messages(request, &cutoff);
    let exists = messages
        .iter()
        .any(|message| string_arg(message, "sentEventID") == root_event_id);
    json!({"messages": messages, "exists": exists})
}

fn pending_friend_messages(request: &Value, cutoff: &str) -> Vec<Value> {
    request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|message| cutoff.is_empty() || string_arg(message, "sentAt").as_str() >= cutoff)
        .collect()
}

// --- Category summaries ---

pub(super) fn category_summaries(request: &Value) -> Value {
    let args = request.get("args").unwrap_or(&Value::Null);
    let include_podcasts = bool_arg_default(args, "include_podcasts", true);
    let limit = bounded_usize_arg(args, "limit", 25, 100);
    let categories_by_id: std::collections::HashMap<String, Value> = request
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|category| (string_arg(category, "id").to_ascii_lowercase(), category.clone()))
        .collect();
    let podcasts_by_id: std::collections::HashMap<String, Value> = request
        .get("podcasts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|podcast| (string_arg(podcast, "podcast_id").to_ascii_lowercase(), podcast.clone()))
        .collect();
    let categories: Vec<Value> = request
        .get("projected_categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(|projected| {
            let category_id = string_arg(projected, "category_id");
            let source = categories_by_id.get(&category_id.to_ascii_lowercase())?;
            let podcast_ids = string_array(projected, "podcast_ids");
            let known_podcast_ids: Vec<&String> = podcast_ids
                .iter()
                .filter(|podcast_id| podcasts_by_id.contains_key(&podcast_id.to_ascii_lowercase()))
                .collect();
            let subscriptions = if include_podcasts {
                known_podcast_ids
                    .iter()
                    .filter_map(|podcast_id| {
                        let podcast = podcasts_by_id.get(&podcast_id.to_ascii_lowercase())?;
                        let mut row = json!({
                            "podcast_id": string_arg(podcast, "podcast_id"),
                            "title": string_arg(podcast, "title"),
                        });
                        insert_optional(
                            &mut row,
                            "author",
                            optional_string_arg(podcast, "author"),
                        );
                        Some(row)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut row = json!({
                "category_id": category_id,
                "name": string_arg(source, "name"),
                "slug": string_arg(source, "slug"),
                "description": string_arg(source, "description"),
                "subscription_count": known_podcast_ids.len(),
                "generated_at": string_arg(source, "generated_at"),
                "subscriptions": subscriptions,
            });
            insert_optional(&mut row, "color_hex", optional_string_arg(source, "color_hex"));
            insert_optional(&mut row, "model", optional_string_arg(source, "model"));
            Some(row)
        })
        .collect();
    json!({"categories": categories})
}

// --- Skill activation and model selection ---

pub(super) fn skill_activation(request: &Value) -> Value {
    let skill_id = string_arg(request, "skill_id");
    let skills = request
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if skill_id.is_empty() {
        return json!({
            "error": "Missing or empty 'skill_id'",
            "enabled_skills": string_array(request, "enabled_skills"),
        });
    }
    let Some(skill) = skills.iter().find(|skill| string_arg(skill, "skill_id") == skill_id) else {
        let known = skills
            .iter()
            .map(|skill| string_arg(skill, "skill_id"))
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return json!({
            "error": format!("Unknown skill '{skill_id}'. Known skills: {known}."),
            "enabled_skills": string_array(request, "enabled_skills"),
        });
    };
    let mut enabled = string_array(request, "enabled_skills");
    let already_enabled = enabled.iter().any(|id| id == &skill_id);
    if !already_enabled {
        enabled.push(skill_id.clone());
    }
    let mut result = json!({
        "success": true,
        "skill_id": skill_id,
        "display_name": string_arg(skill, "display_name"),
        "already_enabled": already_enabled,
        "tools_unlocked": skill.get("tool_names").cloned().unwrap_or_else(|| json!([])),
        "enabled_skills": enabled,
    });
    if !already_enabled {
        result["manual"] = json!(string_arg(skill, "manual"));
    }
    result
}

pub(super) fn local_model_selection(request: &Value) -> Value {
    for key in [
        "agent_initial_model",
        "agent_thinking_model",
        "categorization_model",
        "chapter_compilation_model",
    ] {
        if let Some(model_id) = local_model_id(&string_arg(request, key)) {
            return json!({"model_id": model_id});
        }
    }
    json!({"model_id": null})
}

fn local_model_id(stored_id: &str) -> Option<String> {
    let trimmed = stored_id.trim();
    let rest = trimmed.strip_prefix("local:")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

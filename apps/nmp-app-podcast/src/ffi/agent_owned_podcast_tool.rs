//! Rust-owned policy for agent-owned podcast tools.
//!
//! Swift executes storage, image generation, Blossom upload, and Nostr publish
//! capabilities. Rust owns argument normalization, defaults, visibility
//! fallback, validation wording, row shaping, and final tool envelopes.

use std::ffi::{c_char, CStr, CString};

use serde_json::{json, Value};
use uuid::Uuid;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_owned_podcast_tool(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_owned_podcast_tool",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: Value = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(json!({"error": "Invalid owned-podcast tool request"})),
            };
            encode(dispatch(request))
        },
    )
}

fn dispatch(request: Value) -> Value {
    match string_arg(&request, "op").as_str() {
        "create_plan" => create_plan(&request["args"]),
        "create_lifecycle_plan" => create_lifecycle_plan(&request),
        "mutation_preflight" => mutation_preflight(&request),
        "update_plan" => update_plan(&request["args"]),
        "delete_plan" => required_arg_plan(&request["args"], "podcast_id", "Missing or empty 'podcast_id'"),
        "publish_plan" => required_arg_plan(&request["args"], "episode_id", "Missing or empty 'episode_id'"),
        "artwork_plan" => required_arg_plan(&request["args"], "prompt", "Missing or empty 'prompt'"),
        "info_result" => info_result(&request["podcast"]),
        "list_result" => list_result(&request["podcasts"]),
        "delete_result" => delete_result(&request),
        "publish_result" => publish_result(&request),
        "artwork_result" => artwork_result(&request),
        _ => json!({"error": "Unknown owned-podcast tool operation"}),
    }
}

fn create_plan(args: &Value) -> Value {
    let title = string_arg(args, "title");
    if title.is_empty() {
        return json!({"error": "Missing or empty 'title'"});
    }
    json!({
        "title": title,
        "description": string_arg(args, "description"),
        "author": string_arg(args, "author"),
        "language": optional_string_arg(args, "language"),
        "categories": string_array_arg(args, "categories"),
        "image_url": optional_string_arg(args, "image_url"),
        "visibility": visibility_arg(args, "private"),
    })
}

fn create_lifecycle_plan(request: &Value) -> Value {
    let visibility = visibility_arg(request, "private");
    let active_pubkey = optional_string_arg(request, "active_pubkey");
    if visibility == "public" && active_pubkey.is_none() {
        return json!({"error": "No Nostr signing key configured. Set up your identity in Settings > Agent > Identity."});
    }
    json!({
        "owner_pubkey_hex": active_pubkey.unwrap_or_else(|| "agent-private".into()),
        "should_publish_show": visibility == "public" && bool_arg(request, "nostr_enabled"),
    })
}

fn mutation_preflight(request: &Value) -> Value {
    let podcast_id = string_arg(request, "podcast_id");
    if Uuid::parse_str(&podcast_id).is_err() {
        return json!({"error": format!("Invalid UUID: {podcast_id}")});
    }
    if !bool_arg(request, "exists") {
        return json!({"error": format!("Podcast not found: {podcast_id}")});
    }
    if !bool_arg(request, "is_owned") {
        return json!({"error": format!("Podcast {podcast_id} is not agent-owned.")});
    }
    json!({"ok": true})
}

fn update_plan(args: &Value) -> Value {
    let podcast_id = string_arg(args, "podcast_id");
    if podcast_id.is_empty() {
        return json!({"error": "Missing or empty 'podcast_id'"});
    }
    json!({
        "podcast_id": podcast_id,
        "title": optional_string_arg(args, "title"),
        "description": optional_string_arg(args, "description"),
        "author": optional_string_arg(args, "author"),
        "image_url": optional_string_arg(args, "image_url"),
        "visibility": optional_visibility_arg(args),
    })
}

fn required_arg_plan(args: &Value, key: &str, message: &str) -> Value {
    let value = string_arg(args, key);
    if value.is_empty() {
        return json!({"error": message});
    }
    json!({ key: value })
}

fn info_result(podcast: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(podcast, "podcast_id"),
        "title": string_arg(podcast, "title"),
        "description": string_arg(podcast, "description"),
        "author": string_arg(podcast, "author"),
        "visibility": string_arg(podcast, "visibility"),
        "episode_count": usize_arg(podcast, "episode_count"),
    });
    insert_optional(&mut out, "image_url", optional_string_arg(podcast, "image_url"));
    insert_optional(&mut out, "nostr_event_id", optional_string_arg(podcast, "nostr_event_id"));
    insert_optional(&mut out, "naddr", optional_string_arg(podcast, "naddr"));
    if let Some(count) = podcast.get("episodes_published_to_nostr").and_then(Value::as_u64) {
        out["episodes_published_to_nostr"] = json!(count);
    }
    out
}

fn list_result(podcasts: &Value) -> Value {
    let rows: Vec<Value> = podcasts
        .as_array()
        .into_iter()
        .flatten()
        .map(|podcast| {
            let mut row = info_result(podcast);
            if let Some(object) = row.as_object_mut() {
                object.remove("success");
            }
            row
        })
        .collect();
    let count = rows.len();
    json!({"success": true, "count": count, "podcasts": rows})
}

fn delete_result(request: &Value) -> Value {
    json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "deleted": true,
    })
}

fn publish_result(request: &Value) -> Value {
    let episode_id = string_arg(request, "episode_id");
    match optional_string_arg(request, "naddr") {
        Some(naddr) => json!({"success": true, "episode_id": episode_id, "naddr": naddr}),
        None => json!({
            "error": format!(
                "Episode '{episode_id}' was not published - verify the podcast is agent-owned, its visibility is 'public', and Nostr is enabled in Settings."
            )
        }),
    }
}

fn artwork_result(request: &Value) -> Value {
    json!({
        "success": true,
        "image_url": string_arg(request, "image_url"),
        "prompt": string_arg(request, "prompt"),
    })
}

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    let value = string_arg(args, key);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn usize_arg(args: &Value, key: &str) -> usize {
    args.get(key).and_then(Value::as_u64).unwrap_or_default() as usize
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or_default()
}

fn string_array_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn visibility_arg(args: &Value, default_value: &str) -> String {
    match string_arg(args, "visibility").as_str() {
        "public" => "public".into(),
        "private" => "private".into(),
        _ => default_value.into(),
    }
}

fn optional_visibility_arg(args: &Value) -> Option<String> {
    match optional_string_arg(args, "visibility").as_deref() {
        Some("public") => Some("public".into()),
        Some("private") => Some("private".into()),
        _ => None,
    }
}

fn insert_optional(out: &mut Value, key: &str, value: Option<String>) {
    if let Some(value) = value {
        out[key] = json!(value);
    }
}

fn encode(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

//! Rust-owned simple inventory agent-tool result shaping.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
struct InventoryListRequest {
    op: String,
    #[serde(default)]
    args: Value,
    #[serde(default)]
    podcasts: Vec<PodcastRow>,
    #[serde(default)]
    subscriptions: Vec<SubscriptionRow>,
    #[serde(default)]
    episodes: Vec<EpisodeRow>,
}

#[derive(Debug, Deserialize)]
struct PodcastRow {
    podcast_id: String,
    title: String,
    #[serde(default)]
    author: Option<String>,
    subscribed: bool,
    total_episodes: usize,
    unplayed_episodes: usize,
    #[serde(default)]
    last_published_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionRow {
    podcast_id: String,
    title: String,
    #[serde(default)]
    author: Option<String>,
    total_episodes: usize,
    unplayed_episodes: usize,
    #[serde(default)]
    last_published_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct EpisodeRow {
    episode_id: String,
    podcast_id: String,
    title: String,
    podcast_title: String,
    #[serde(default)]
    published_at: Option<i64>,
    #[serde(default)]
    duration_seconds: Option<i64>,
    played: bool,
    playback_position_seconds: f64,
    is_in_progress: bool,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_inventory_list(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_inventory_list",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: InventoryListRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_json(json!({"error": "Invalid inventory-list request"})),
            };
            encode_json(inventory_list(request))
        },
    )
}

fn inventory_list(request: InventoryListRequest) -> Value {
    let limit = limit_arg(&request.args);
    match request.op.as_str() {
        "list_podcasts" => {
            let rows: Vec<Value> = request
                .podcasts
                .into_iter()
                .take(limit)
                .map(serialize_podcast)
                .collect();
            let count = rows.len();
            json!({"success": true, "podcasts": rows, "count": count})
        }
        "list_subscriptions" => {
            let rows: Vec<Value> = request
                .subscriptions
                .into_iter()
                .take(limit)
                .map(serialize_subscription)
                .collect();
            let count = rows.len();
            json!({"success": true, "subscriptions": rows, "count": count})
        }
        "list_in_progress" | "list_recent_unplayed" => {
            let rows: Vec<Value> = request
                .episodes
                .into_iter()
                .take(limit)
                .map(serialize_episode)
                .collect();
            let count = rows.len();
            json!({"success": true, "episodes": rows, "count": count})
        }
        _ => json!({"error": "Unknown inventory-list operation"}),
    }
}

fn serialize_podcast(row: PodcastRow) -> Value {
    let mut out = json!({
        "podcast_id": row.podcast_id,
        "title": row.title,
        "subscribed": row.subscribed,
        "total_episodes": row.total_episodes,
        "unplayed_episodes": row.unplayed_episodes,
    });
    insert_if_present(&mut out, "author", row.author.as_deref());
    insert_timestamp(&mut out, "last_published_at", row.last_published_at);
    out
}

fn serialize_subscription(row: SubscriptionRow) -> Value {
    let mut out = json!({
        "podcast_id": row.podcast_id,
        "title": row.title,
        "total_episodes": row.total_episodes,
        "unplayed_episodes": row.unplayed_episodes,
    });
    insert_if_present(&mut out, "author", row.author.as_deref());
    insert_timestamp(&mut out, "last_published_at", row.last_published_at);
    out
}

fn serialize_episode(row: EpisodeRow) -> Value {
    let mut out = json!({
        "episode_id": row.episode_id,
        "podcast_id": row.podcast_id,
        "title": row.title,
        "podcast_title": row.podcast_title,
        "played": row.played,
        "playback_position_seconds": row.playback_position_seconds,
        "is_in_progress": row.is_in_progress,
    });
    insert_timestamp(&mut out, "published_at", row.published_at);
    if let Some(duration) = row.duration_seconds {
        out["duration_seconds"] = json!(duration);
    }
    out
}

fn insert_if_present(row: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        row[key] = json!(value);
    }
}

fn insert_timestamp(row: &mut Value, key: &str, timestamp: Option<i64>) {
    if let Some(timestamp) = timestamp.and_then(|ts| Utc.timestamp_opt(ts, 0).single()) {
        row[key] = json!(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
}

fn limit_arg(args: &Value) -> usize {
    let parsed = match args.get("limit") {
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize),
        Some(Value::String(s)) => s.trim().parse::<usize>().ok(),
        _ => None,
    }
    .unwrap_or(DEFAULT_LIMIT);
    parsed.clamp(1, MAX_LIMIT)
}

fn encode_json(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

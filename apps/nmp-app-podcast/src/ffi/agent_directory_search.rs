//! Rust-owned `search_podcast_directory` agent-tool policy.
//!
//! Swift executes the directory capability. Rust owns argument normalization,
//! type fallback, limit caps, final row shaping, and result counters.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;

#[derive(Debug, Deserialize)]
struct DirectoryPlanRequest {
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct DirectoryPlanResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DirectoryResultsRequest {
    query: String,
    search_type: String,
    #[serde(default)]
    results: Vec<DirectoryHitRow>,
}

#[derive(Debug, Deserialize)]
struct DirectoryHitRow {
    #[serde(default)]
    collection_id: Option<i64>,
    podcast_title: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    feed_url: Option<String>,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    episode_title: Option<String>,
    #[serde(default)]
    audio_url: Option<String>,
    #[serde(default)]
    episode_guid: Option<String>,
    #[serde(default)]
    published_at: Option<i64>,
    #[serde(default)]
    duration_seconds: Option<i64>,
    #[serde(default)]
    description: Option<String>,
}

fn encode<T: Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn encode_value(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_directory_search_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_directory_search_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: DirectoryPlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&DirectoryPlanResponse::error("Invalid directory search request")),
            };
            encode(&directory_plan(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_directory_search_results(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_directory_search_results",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: DirectoryResultsRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid directory search results"})),
            };
            encode_value(directory_results(request))
        },
    )
}

fn directory_plan(request: DirectoryPlanRequest) -> DirectoryPlanResponse {
    let query = string_arg(&request.args, "query").trim().to_string();
    if query.is_empty() {
        return DirectoryPlanResponse::error("Missing or empty 'query'");
    }
    DirectoryPlanResponse {
        error: None,
        query: Some(query),
        search_type: Some(search_type_arg(&request.args)),
        limit: Some(limit_arg(&request.args)),
    }
}

fn directory_results(request: DirectoryResultsRequest) -> Value {
    let rows: Vec<Value> = request.results.into_iter().map(serialize_hit).collect();
    json!({
        "success": true,
        "query": request.query,
        "type": request.search_type,
        "total_found": rows.len(),
        "results": rows,
    })
}

fn serialize_hit(hit: DirectoryHitRow) -> Value {
    let mut row = json!({
        "podcast_title": hit.podcast_title,
    });
    if let Some(id) = hit.collection_id {
        row["collection_id"] = json!(id);
    }
    insert_if_present(&mut row, "author", hit.author.as_deref());
    insert_if_present(&mut row, "feed_url", hit.feed_url.as_deref());
    insert_if_present(&mut row, "artwork_url", hit.artwork_url.as_deref());
    insert_if_present(&mut row, "episode_title", hit.episode_title.as_deref());
    insert_if_present(&mut row, "audio_url", hit.audio_url.as_deref());
    insert_if_present(&mut row, "episode_guid", hit.episode_guid.as_deref());
    if let Some(published_at) = hit.published_at.and_then(|ts| Utc.timestamp_opt(ts, 0).single()) {
        row["published_at"] = json!(published_at.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    if let Some(duration) = hit.duration_seconds {
        row["duration_seconds"] = json!(duration);
    }
    insert_if_present(&mut row, "description", hit.description.as_deref());
    row
}

fn insert_if_present(row: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        row[key] = json!(value);
    }
}

fn search_type_arg(args: &Value) -> String {
    match string_arg(args, "type").trim().to_lowercase().as_str() {
        "podcast" => "podcast".into(),
        _ => "episode".into(),
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

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl DirectoryPlanResponse {
    fn error(error: &str) -> Self {
        Self {
            error: Some(error.to_string()),
            query: None,
            search_type: None,
            limit: None,
        }
    }
}

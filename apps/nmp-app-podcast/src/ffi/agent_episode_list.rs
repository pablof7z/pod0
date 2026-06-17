//! Rust-owned `list_episodes` agent-tool policy.
//!
//! Swift executes native/directory/feed capabilities. Rust owns argument
//! validation, identifier interpretation, caps, error wording, and final row
//! shape.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
struct EpisodeListPlanRequest {
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct EpisodeListPlanResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feed_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EpisodeListResultsRequest {
    source: String,
    podcast_id: String,
    #[serde(default)]
    feed_url: Option<String>,
    #[serde(default)]
    podcast_title: Option<String>,
    #[serde(default)]
    episodes: Vec<EpisodeRow>,
}

#[derive(Debug, Deserialize)]
struct EpisodeListErrorRequest {
    kind: String,
    #[serde(default)]
    podcast_id: Option<String>,
    #[serde(default)]
    feed_url: Option<String>,
    #[serde(default)]
    collection_id: Option<String>,
    #[serde(default)]
    detail: Option<String>,
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
pub extern "C" fn nmp_app_podcast_agent_episode_list_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_episode_list_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: EpisodeListPlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&EpisodeListPlanResponse::error("Invalid episode-list request")),
            };
            encode(&episode_list_plan(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_episode_list_results(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_episode_list_results",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: EpisodeListResultsRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid episode-list results"})),
            };
            encode_value(episode_list_results(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_episode_list_error(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_episode_list_error",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: EpisodeListErrorRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid episode-list error"})),
            };
            encode_value(json!({"error": episode_list_error(request)}))
        },
    )
}

fn episode_list_plan(request: EpisodeListPlanRequest) -> EpisodeListPlanResponse {
    let podcast_id = string_arg(&request.args, "podcast_id").trim().to_string();
    let feed_url = string_arg(&request.args, "feed_url").trim().to_string();
    let has_podcast_id = !podcast_id.is_empty();
    let has_feed_url = !feed_url.is_empty();
    match (has_podcast_id, has_feed_url) {
        (false, false) => EpisodeListPlanResponse::error("Provide one of 'podcast_id' or 'feed_url'"),
        (true, true) => EpisodeListPlanResponse::error("Provide only one of 'podcast_id' or 'feed_url', not both"),
        (false, true) => EpisodeListPlanResponse {
            error: None,
            source: Some("feed_url".into()),
            podcast_id: None,
            feed_url: Some(feed_url),
            collection_id: None,
            limit: Some(limit_arg(&request.args)),
        },
        (true, false) if Uuid::parse_str(&podcast_id).is_ok() => EpisodeListPlanResponse {
            error: None,
            source: Some("internal".into()),
            podcast_id: Some(podcast_id),
            feed_url: None,
            collection_id: None,
            limit: Some(limit_arg(&request.args)),
        },
        (true, false) => EpisodeListPlanResponse {
            error: None,
            source: Some("collection_id".into()),
            podcast_id: None,
            feed_url: None,
            collection_id: Some(podcast_id),
            limit: Some(limit_arg(&request.args)),
        },
    }
}

fn episode_list_results(request: EpisodeListResultsRequest) -> Value {
    let rows: Vec<Value> = request.episodes.into_iter().map(serialize_episode).collect();
    let count = rows.len();
    let title = request
        .podcast_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            rows.first()
                .and_then(|row| row.get("podcast_title"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let mut payload = json!({
        "success": true,
        "podcast_id": request.podcast_id,
        "episodes": rows,
        "count": count,
    });
    if let Some(feed_url) = request.feed_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        payload["feed_url"] = json!(feed_url);
    }
    if let Some(title) = title {
        payload["podcast_title"] = json!(title);
    }
    payload
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
    if let Some(published_at) = row.published_at.and_then(|ts| Utc.timestamp_opt(ts, 0).single()) {
        out["published_at"] = json!(published_at.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    if let Some(duration) = row.duration_seconds {
        out["duration_seconds"] = json!(duration);
    }
    out
}

fn episode_list_error(request: EpisodeListErrorRequest) -> String {
    let detail = request.detail.unwrap_or_default();
    match request.kind.as_str() {
        "unknown_podcast" => format!(
            "Unknown podcast: {}",
            request.podcast_id.unwrap_or_default()
        ),
        "collection_lookup_failed" => format!(
            "Could not resolve podcast directory ID '{}': {}",
            request.collection_id.unwrap_or_default(),
            detail
        ),
        "collection_not_found" => format!(
            "Could not resolve podcast directory ID '{}': no matching show in the Apple Podcasts directory",
            request.collection_id.unwrap_or_default()
        ),
        "feed_load_failed" => format!(
            "Could not load feed '{}': {}",
            request.feed_url.unwrap_or_default(),
            detail
        ),
        "feed_row_missing" => format!(
            "Feed '{}' was loaded but its podcast row could not be located in the inventory.",
            request.feed_url.unwrap_or_default()
        ),
        _ => detail,
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

impl EpisodeListPlanResponse {
    fn error(error: &str) -> Self {
        Self {
            error: Some(error.to_string()),
            source: None,
            podcast_id: None,
            feed_url: None,
            collection_id: None,
            limit: None,
        }
    }
}

//! Rust-owned `search_youtube` agent-tool policy.
//!
//! Swift executes the YouTube extractor capability. Rust owns tool arg
//! normalization, limit caps, and final tool-result shaping.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;

#[derive(Debug, Deserialize)]
struct SearchPlanRequest {
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct SearchPlanResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchResultsRequest {
    query: String,
    #[serde(default)]
    results: Vec<YouTubeResultRow>,
}

#[derive(Debug, Deserialize)]
struct YouTubeResultRow {
    url: String,
    title: String,
    author: String,
    #[serde(default)]
    duration_seconds: Option<f64>,
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
pub extern "C" fn nmp_app_podcast_agent_youtube_search_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_youtube_search_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: SearchPlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&SearchPlanResponse::error("Invalid YouTube search request")),
            };
            encode(&search_plan(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_youtube_search_results(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_youtube_search_results",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: SearchResultsRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid YouTube search results"})),
            };
            encode_value(search_results(request))
        },
    )
}

fn search_plan(request: SearchPlanRequest) -> SearchPlanResponse {
    let query = string_arg(&request.args, "query").trim().to_string();
    if query.is_empty() {
        return SearchPlanResponse::error("Missing or empty 'query'");
    }
    SearchPlanResponse {
        error: None,
        query: Some(query),
        limit: Some(limit_arg(&request.args)),
    }
}

fn search_results(request: SearchResultsRequest) -> Value {
    let rows: Vec<Value> = request
        .results
        .into_iter()
        .map(|result| {
            let mut row = json!({
                "url": result.url,
                "title": result.title,
                "author": result.author,
            });
            if let Some(duration) = result.duration_seconds {
                row["duration_seconds"] = json!(duration);
            }
            row
        })
        .collect();
    json!({
        "success": true,
        "query": request.query,
        "total_found": rows.len(),
        "results": rows,
    })
}

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

impl SearchPlanResponse {
    fn error(error: &str) -> Self {
        Self {
            error: Some(error.to_string()),
            query: None,
            limit: None,
        }
    }
}

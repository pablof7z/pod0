//! Rust-owned search/RAG agent-tool policy.
//!
//! Swift executes the RAG capability. Rust owns argument validation, caps,
//! query/scope normalization, timestamp formatting, row shaping, and tool
//! envelopes for search tools.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const SEARCH_DEFAULT_LIMIT: usize = 10;
const SEARCH_MAX_LIMIT: usize = 25;
const TRANSCRIPT_DEFAULT_LIMIT: usize = 8;
const SIMILAR_DEFAULT_K: usize = 5;
const SIMILAR_MAX_K: usize = 20;

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_search_tool(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_search_tool",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: Value = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(json!({"error": "Invalid search tool request"})),
            };
            encode(dispatch(request))
        },
    )
}

fn dispatch(request: Value) -> Value {
    match string_arg(&request, "op").as_str() {
        "search_plan" => episode_search_plan(&request["args"]),
        "transcript_plan" => query_plan(&request["args"], TRANSCRIPT_DEFAULT_LIMIT, SEARCH_MAX_LIMIT, "query"),
        "similar_plan" => similar_plan(&request["args"]),
        "episode_results" => episode_results(&request),
        "transcript_results" => transcript_results(&request),
        "transcript_hits" => transcript_hits(&request),
        "perplexity_plan" => query_plan(&request["args"], SEARCH_DEFAULT_LIMIT, SEARCH_MAX_LIMIT, "query"),
        "perplexity_results" => perplexity_results(&request),
        "summary_plan" => required_arg_plan(&request["args"], "episode_id", "Missing or empty 'episode_id'"),
        "summary_result" => json!({
            "success": true,
            "episode_id": string_arg(&request, "episode_id"),
            "summary": string_arg(&request, "summary"),
        }),
        "episode_rollup" => episode_rollup(&request),
        _ => json!({"error": "Unknown search tool operation"}),
    }
}

fn required_arg_plan(args: &Value, key: &str, message: &str) -> Value {
    let value = string_arg(args, key);
    if value.is_empty() {
        return json!({"error": message});
    }
    json!({ key: value })
}

fn query_plan(args: &Value, default_limit: usize, max_limit: usize, query_key: &str) -> Value {
    let query = string_arg(args, query_key);
    if query.is_empty() {
        return json!({"error": "Missing or empty 'query'"});
    }
    json!({
        "query": query,
        "scope": optional_string_arg(args, "scope"),
        "limit": limit_arg(args, "limit", default_limit, max_limit),
    })
}

fn episode_search_plan(args: &Value) -> Value {
    let query = string_arg(args, "query");
    if query.is_empty() {
        return json!({"error": "Missing or empty 'query'"});
    }
    let limit = limit_arg(args, "limit", SEARCH_DEFAULT_LIMIT, SEARCH_MAX_LIMIT);
    json!({
        "query": query,
        "scope": optional_string_arg(args, "scope"),
        "limit": limit,
        "retrieval_limit": (limit * 4).clamp(1, SEARCH_MAX_LIMIT * 4),
    })
}

fn similar_plan(args: &Value) -> Value {
    let seed = string_arg(args, "seed_episode_id");
    if seed.is_empty() {
        return json!({"error": "Missing or empty 'seed_episode_id'"});
    }
    json!({
        "seed_episode_id": seed,
        "k": limit_arg(args, "k", SIMILAR_DEFAULT_K, SIMILAR_MAX_K),
    })
}

fn episode_results(request: &Value) -> Value {
    let rows: Vec<Value> = request
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(episode_row)
        .collect();
    let total_found = rows.len();
    let mut out = json!({
        "success": true,
        "total_found": total_found,
        "results": rows,
    });
    insert_optional(&mut out, "query", optional_string_arg(request, "query"));
    insert_optional(&mut out, "seed_episode_id", optional_string_arg(request, "seed_episode_id"));
    if let Some(k) = request.get("k").and_then(Value::as_u64) {
        out["k"] = json!(k);
    }
    out
}

fn transcript_results(request: &Value) -> Value {
    let rows: Vec<Value> = request
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(transcript_row)
        .collect();
    let total_found = rows.len();
    json!({
        "success": true,
        "query": string_arg(request, "query"),
        "total_found": total_found,
        "results": rows,
    })
}

fn transcript_hits(request: &Value) -> Value {
    let rows: Vec<Value> = request
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let mut out = json!({
                "episode_id": string_arg(row, "episode_id"),
                "start_seconds": row.get("start_secs").and_then(Value::as_f64).unwrap_or_default(),
                "end_seconds": row.get("end_secs").and_then(Value::as_f64).unwrap_or_default(),
                "text": string_arg(row, "text"),
            });
            if let Some(score) = row.get("relevance_score").and_then(Value::as_f64) {
                out["score"] = json!(score);
            }
            out
        })
        .collect();
    json!({"result": rows})
}

fn perplexity_results(request: &Value) -> Value {
    let sources: Vec<Value> = request
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|source| {
            json!({
                "title": string_arg(source, "title"),
                "url": string_arg(source, "url"),
            })
        })
        .collect();
    json!({
        "success": true,
        "query": string_arg(request, "query"),
        "answer": string_arg(request, "answer"),
        "sources": sources,
    })
}

fn episode_rollup(request: &Value) -> Value {
    let limit = request
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(SEARCH_DEFAULT_LIMIT)
        .clamp(1, SEARCH_MAX_LIMIT);
    let rows = request
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let metadata = metadata_map(request.get("metadata"));
    let mut ordered_ids: Vec<String> = Vec::new();
    let mut best: std::collections::HashMap<String, Value> = std::collections::HashMap::new();

    for row in rows {
        let episode_id = string_arg(&row, "episode_id");
        if episode_id.is_empty() {
            continue;
        }
        if let Some(prior) = best.get(&episode_id) {
            if number_arg(&row, "relevance_score") > number_arg(prior, "relevance_score") {
                best.insert(episode_id, row);
            }
        } else {
            ordered_ids.push(episode_id.clone());
            best.insert(episode_id, row);
        }
        if ordered_ids.len() >= limit {
            break;
        }
    }

    let result = ordered_ids
        .into_iter()
        .take(limit)
        .filter_map(|id| {
            let row = best.get(&id)?;
            let meta = metadata.get(&id);
            let mut out = json!({
                "episode_id": string_arg(row, "episode_id"),
                "podcast_id": string_arg(row, "podcast_id"),
                "title": string_arg(row, "episode_title"),
                "podcast_title": string_arg(row, "podcast_title"),
                "snippet": truncate(&string_arg(row, "text"), 280),
                "score": number_arg(row, "relevance_score"),
            });
            if let Some(meta) = meta {
                if let Some(published_at) = meta.get("published_at").and_then(Value::as_i64) {
                    out["published_at"] = json!(published_at);
                }
                if let Some(duration) = meta.get("duration_seconds").and_then(Value::as_i64) {
                    out["duration_seconds"] = json!(duration);
                }
            }
            Some(out)
        })
        .collect::<Vec<_>>();

    json!({ "result": result })
}

fn metadata_map(raw: Option<&Value>) -> std::collections::HashMap<String, Value> {
    raw.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let episode_id = string_arg(row, "episode_id");
            if episode_id.is_empty() {
                None
            } else {
                Some((episode_id, row.clone()))
            }
        })
        .collect()
}

fn episode_row(row: &Value) -> Value {
    let mut out = json!({
        "episode_id": string_arg(row, "episode_id"),
        "podcast_id": string_arg(row, "podcast_id"),
        "title": string_arg(row, "title"),
        "podcast_title": string_arg(row, "podcast_title"),
    });
    if let Some(timestamp) = row.get("published_at").and_then(Value::as_i64) {
        if let Some(datetime) = Utc.timestamp_opt(timestamp, 0).single() {
            out["published_at"] = json!(datetime.to_rfc3339_opts(SecondsFormat::Secs, true));
        }
    }
    if let Some(duration) = row.get("duration_seconds").and_then(Value::as_i64) {
        out["duration_seconds"] = json!(duration);
    }
    insert_optional(&mut out, "snippet", optional_string_arg(row, "snippet"));
    if let Some(score) = row.get("score").and_then(Value::as_f64) {
        out["score"] = json!(score);
    }
    out
}

fn transcript_row(row: &Value) -> Value {
    let mut out = json!({
        "episode_id": string_arg(row, "episode_id"),
        "start_seconds": row.get("start_seconds").and_then(Value::as_f64).unwrap_or_default(),
        "end_seconds": row.get("end_seconds").and_then(Value::as_f64).unwrap_or_default(),
        "text": string_arg(row, "text"),
    });
    insert_optional(&mut out, "speaker", optional_string_arg(row, "speaker"));
    if let Some(score) = row.get("score").and_then(Value::as_f64) {
        out["score"] = json!(score);
    }
    out
}

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn number_arg(args: &Value, key: &str) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    let value = string_arg(args, key);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn limit_arg(args: &Value, key: &str, default_value: usize, max: usize) -> usize {
    let parsed = match args.get(key) {
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize),
        Some(Value::String(s)) => s.trim().parse::<usize>().ok(),
        _ => None,
    }
    .unwrap_or(default_value);
    parsed.clamp(1, max)
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

//! Rust-owned `list_categories` agent-tool shaping.
//!
//! Swift gets category summaries from the already Rust-owned category/library
//! projection. Rust owns tool arg normalization, caps, include-podcasts
//! handling, row shaping, and counters.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
struct CategoryListRequest {
    #[serde(default)]
    args: Value,
    #[serde(default)]
    categories: Vec<CategoryRow>,
}

#[derive(Debug, Deserialize)]
struct CategoryRow {
    category_id: String,
    name: String,
    slug: String,
    description: String,
    #[serde(default)]
    color_hex: Option<String>,
    subscription_count: usize,
    generated_at: i64,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    subscriptions: Vec<CategorySubscriptionRow>,
}

#[derive(Debug, Deserialize)]
struct CategorySubscriptionRow {
    podcast_id: String,
    title: String,
    #[serde(default)]
    author: Option<String>,
}

fn encode_json(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_category_list(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_category_list",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: CategoryListRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_json(json!({"error": "Invalid category-list request"})),
            };
            encode_json(list_categories(request))
        },
    )
}

fn list_categories(request: CategoryListRequest) -> Value {
    let include_podcasts = bool_arg(&request.args, "include_podcasts", true);
    let limit = limit_arg(&request.args);
    let categories: Vec<Value> = request
        .categories
        .into_iter()
        .take(limit)
        .map(|category| serialize_category(category, include_podcasts))
        .collect();
    let count = categories.len();
    json!({
        "success": true,
        "categories": categories,
        "count": count,
    })
}

fn serialize_category(category: CategoryRow, include_podcasts: bool) -> Value {
    let mut out = json!({
        "category_id": category.category_id,
        "name": category.name,
        "slug": category.slug,
        "description": category.description,
        "subscription_count": category.subscription_count,
        "generated_at": format_timestamp(category.generated_at),
    });
    insert_if_present(&mut out, "color_hex", category.color_hex.as_deref());
    insert_if_present(&mut out, "model", category.model.as_deref());
    if include_podcasts {
        out["subscriptions"] = json!(
            category
                .subscriptions
                .into_iter()
                .map(serialize_subscription)
                .collect::<Vec<_>>()
        );
    }
    out
}

fn serialize_subscription(subscription: CategorySubscriptionRow) -> Value {
    let mut out = json!({
        "podcast_id": subscription.podcast_id,
        "title": subscription.title,
    });
    insert_if_present(&mut out, "author", subscription.author.as_deref());
    out
}

fn insert_if_present(row: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        row[key] = json!(value);
    }
}

fn format_timestamp(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
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

fn bool_arg(args: &Value, key: &str, default_value: bool) -> bool {
    match args.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(n)) => n.as_i64().map(|v| v != 0).unwrap_or(default_value),
        Some(Value::String(s)) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => true,
            "false" | "no" | "0" => false,
            _ => default_value,
        },
        _ => default_value,
    }
}

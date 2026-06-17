//! Rust-owned `list_available_voices` agent-tool shaping.
//!
//! Swift fetches the ElevenLabs catalog through the existing Rust transport and
//! passes raw voice rows here. Rust owns query matching, caps, row shaping, and
//! tool-result counters.

use std::ffi::{c_char, CStr, CString};

use serde::Deserialize;
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const DEFAULT_LIMIT: usize = 30;
const MAX_LIMIT: usize = 50;

#[derive(Debug, Deserialize)]
struct VoiceListRequest {
    #[serde(default)]
    args: Value,
    #[serde(default)]
    voices: Vec<VoiceRow>,
}

#[derive(Debug, Deserialize)]
struct VoiceRow {
    voice_id: String,
    name: String,
    category: String,
    #[serde(default)]
    gender: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    age: Option<String>,
    #[serde(default)]
    use_case: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default)]
    labels: std::collections::HashMap<String, String>,
}

fn encode_json(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_voice_list(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_voice_list",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: VoiceListRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_json(json!({"error": "Invalid voice-list request"})),
            };
            encode_json(list_available_voices(request))
        },
    )
}

fn list_available_voices(request: VoiceListRequest) -> Value {
    let query = string_arg(&request.args, "query").trim().to_lowercase();
    let limit = limit_arg(&request.args);
    let total_available = request.voices.len();
    let matched: Vec<&VoiceRow> = request
        .voices
        .iter()
        .filter(|voice| query.is_empty() || search_text(voice).contains(&query))
        .collect();
    let results: Vec<Value> = matched
        .iter()
        .take(limit)
        .map(|voice| serialize_voice(voice))
        .collect();

    json!({
        "success": true,
        "total_available": total_available,
        "total_matched": matched.len(),
        "results": results,
    })
}

fn serialize_voice(voice: &VoiceRow) -> Value {
    let mut row = json!({
        "voice_id": voice.voice_id,
        "name": voice.name,
        "category": voice.category,
    });
    insert_if_present(&mut row, "gender", voice.gender.as_deref());
    insert_if_present(&mut row, "accent", voice.accent.as_deref());
    insert_if_present(&mut row, "age", voice.age.as_deref());
    insert_if_present(&mut row, "use_case", voice.use_case.as_deref());
    insert_if_present(&mut row, "description", voice.description.as_deref());
    insert_if_present(&mut row, "preview_url", voice.preview_url.as_deref());
    row
}

fn insert_if_present(row: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        row[key] = json!(value);
    }
}

fn search_text(voice: &VoiceRow) -> String {
    let mut parts = vec![
        voice.name.as_str(),
        voice.voice_id.as_str(),
        voice.category.as_str(),
    ];
    parts.extend(voice.labels.values().map(String::as_str));
    for value in [
        voice.gender.as_deref(),
        voice.accent.as_deref(),
        voice.age.as_deref(),
        voice.use_case.as_deref(),
        voice.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        parts.push(value);
    }
    parts.join(" ").to_lowercase()
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

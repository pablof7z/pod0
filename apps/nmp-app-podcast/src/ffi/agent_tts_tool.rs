//! Rust-owned agent TTS tool policy.
//!
//! Swift executes native/cloud capabilities: owned-podcast lookup, audio
//! synthesis, stitching, publishing, and playback. Rust owns tool argument
//! validation, owned-show gating, turn normalization, defaulting, and response
//! shaping.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct TTSToolPlanRequest {
    #[serde(default)]
    args: Value,
    #[serde(default)]
    owned_podcast_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TTSToolPlanResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default)]
    play_now: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_podcast_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    turns: Vec<TTSTurnPlan>,
}

#[derive(Debug, Serialize)]
struct TTSTurnPlan {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TTSToolResultRequest {
    episode_id: String,
    podcast_id: String,
    title: String,
    published_to_library: bool,
    turn_count: usize,
    #[serde(default)]
    duration_seconds: Option<f64>,
    #[serde(default)]
    play_now: bool,
}

#[derive(Debug, Deserialize)]
struct VoiceConfigurePlanRequest {
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct VoiceConfigurePlanResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VoiceConfigureResultRequest {
    voice_id: String,
    #[serde(default)]
    previous_voice_id: Option<String>,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_tts_tool_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_tts_tool_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TTSToolPlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&TTSToolPlanResponse::error("Invalid TTS episode request")),
            };
            encode(&tts_tool_plan(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_tts_tool_result(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_tts_tool_result",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TTSToolResultRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid TTS episode result"})),
            };
            encode_value(tts_tool_result(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_voice_configure_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_voice_configure_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: VoiceConfigurePlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&VoiceConfigurePlanResponse::error("Invalid voice configuration request")),
            };
            encode(&voice_configure_plan(request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_voice_configure_result(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_voice_configure_result",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: VoiceConfigureResultRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode_value(json!({"error": "Invalid voice configuration result"})),
            };
            encode_value(voice_configure_result(request))
        },
    )
}

fn tts_tool_plan(request: TTSToolPlanRequest) -> TTSToolPlanResponse {
    let title = string_arg(&request.args, "title").trim().to_string();
    if title.is_empty() {
        return TTSToolPlanResponse::error("Missing or empty 'title'");
    }
    let turns_value = request.args.get("turns").and_then(Value::as_array);
    let Some(raw_turns) = turns_value.filter(|turns| !turns.is_empty()) else {
        return TTSToolPlanResponse::error("'turns' must be a non-empty array");
    };
    let target_podcast_id = match optional_string_arg(&request.args, "podcast_id") {
        Some(raw) => {
            if Uuid::parse_str(&raw).is_err() {
                return TTSToolPlanResponse::error(&format!("podcast_id '{raw}' is not a valid UUID"));
            }
            if !request.owned_podcast_ids.iter().any(|id| id == &raw) {
                return TTSToolPlanResponse::error(&format!(
                    "podcast_id '{raw}' is not an agent-owned podcast - use list_my_podcasts to find valid IDs"
                ));
            }
            Some(raw)
        }
        None => None,
    };
    let mut turns = Vec::with_capacity(raw_turns.len());
    for (index, raw_turn) in raw_turns.iter().enumerate() {
        let Some(turn) = parse_turn(index, raw_turn) else {
            return TTSToolPlanResponse::error(&turn_error(index, raw_turn));
        };
        turns.push(turn);
    }
    TTSToolPlanResponse {
        error: None,
        title: Some(title),
        description: optional_string_arg(&request.args, "description"),
        play_now: bool_arg(&request.args, "play_now", false),
        target_podcast_id,
        turns,
    }
}

fn parse_turn(index: usize, raw_turn: &Value) -> Option<TTSTurnPlan> {
    let kind = string_arg(raw_turn, "kind").trim().to_lowercase();
    match kind.as_str() {
        "speech" => {
            let text = optional_string_arg(raw_turn, "text")?;
            Some(TTSTurnPlan {
                kind,
                text: Some(text),
                voice_id: optional_string_arg(raw_turn, "voice_id"),
                episode_id: None,
                start_seconds: None,
                end_seconds: None,
                label: None,
            })
        }
        "snippet" => {
            let episode_id = optional_string_arg(raw_turn, "episode_id")?;
            let start = numeric_arg(raw_turn, "start_seconds")?;
            let end = numeric_arg(raw_turn, "end_seconds")?;
            if end <= start {
                return None;
            }
            Some(TTSTurnPlan {
                kind,
                text: None,
                voice_id: None,
                episode_id: Some(episode_id),
                start_seconds: Some(start),
                end_seconds: Some(end),
                label: optional_string_arg(raw_turn, "label"),
            })
        }
        _ if kind.is_empty() => {
            let _ = index;
            None
        }
        _ => None,
    }
}

fn turn_error(index: usize, raw_turn: &Value) -> String {
    let kind = string_arg(raw_turn, "kind").trim().to_lowercase();
    match kind.as_str() {
        "" => format!("Turn {index}: missing 'kind' (speech | snippet)"),
        "speech" => format!("Turn {index}: speech turn requires non-empty 'text'"),
        "snippet" if optional_string_arg(raw_turn, "episode_id").is_none() => {
            format!("Turn {index}: snippet turn requires 'episode_id'")
        }
        "snippet" => format!("Turn {index}: snippet turn requires valid 'start_seconds' < 'end_seconds'"),
        _ => format!("Turn {index}: unknown kind '{kind}' - must be 'speech' or 'snippet'"),
    }
}

fn tts_tool_result(request: TTSToolResultRequest) -> Value {
    let mut payload = json!({
        "success": true,
        "episode_id": request.episode_id,
        "podcast_id": request.podcast_id,
        "title": request.title,
        "published_to_library": request.published_to_library,
        "turn_count": request.turn_count,
    });
    if let Some(duration) = request.duration_seconds {
        payload["duration_seconds"] = json!(duration as i64);
    }
    if request.play_now {
        payload["play_now"] = json!(true);
    }
    payload
}

fn voice_configure_plan(request: VoiceConfigurePlanRequest) -> VoiceConfigurePlanResponse {
    let voice_id = string_arg(&request.args, "voice_id").trim().to_string();
    if voice_id.is_empty() {
        return VoiceConfigurePlanResponse::error("Missing or empty 'voice_id'");
    }
    VoiceConfigurePlanResponse {
        error: None,
        voice_id: Some(voice_id),
    }
}

fn voice_configure_result(request: VoiceConfigureResultRequest) -> Value {
    let mut payload = json!({
        "success": true,
        "voice_id": request.voice_id,
    });
    if let Some(previous) = request.previous_voice_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        payload["previous_voice_id"] = json!(previous);
    }
    payload
}

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn numeric_arg(args: &Value, key: &str) -> Option<f64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
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

impl TTSToolPlanResponse {
    fn error(error: &str) -> Self {
        Self {
            error: Some(error.to_string()),
            title: None,
            description: None,
            play_now: false,
            target_podcast_id: None,
            turns: Vec::new(),
        }
    }
}

impl VoiceConfigurePlanResponse {
    fn error(error: &str) -> Self {
        Self {
            error: Some(error.to_string()),
            voice_id: None,
        }
    }
}

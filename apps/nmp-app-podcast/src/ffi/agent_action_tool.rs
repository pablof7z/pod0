//! Rust-owned final envelopes for podcast action tools.
//!
//! Swift executes playback, library, transcript, and clipping capabilities.
//! Rust owns the agent-facing JSON result shape and derived labels/messages.
//!
//! ## Module layout
//!
//! | Submodule | Contents |
//! |---|---|
//! | [`plans`] | `*_plan` validation / plan-building functions |
//! | [`results`] | `*_result` / `*_snapshot` / `*_status` / `*_label` projections |
//! | [`data`] | Chat-history, agent-activity/run ledgers, pending-friend, category summaries |

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

mod data;
mod plans;
mod results;
#[cfg(test)]
mod tests;

use data::{
    agent_activity_active_count, agent_activity_for_batch, agent_activity_mark_undone,
    agent_activity_prune, agent_activity_record, agent_activity_sorted,
    agent_activity_undo_batch_ids, agent_run_filter, agent_run_normalize, agent_run_record,
    category_summaries, chat_history_normalize, chat_history_upsert, chat_history_wrap_legacy,
    local_model_selection, pending_friend_claim, pending_friend_has, pending_friend_register,
    skill_activation,
};
use plans::{
    ask_plan, category_assignments_plan, category_change_plan,
    category_transcription_disabled_plan, clip_plan, download_transcribe_plan, peer_end_plan,
    peer_message_plan, play_plan, rate_plan, schedule_plan, seek_plan, sleep_plan,
    tts_publish_plan, youtube_ingest_plan,
};
use results::{
    category_change_result, clip_result, delete_podcast_result, delete_podcast_snapshot,
    episode_mutation_result, episode_summary_policy, memory_result, nostr_peer_label, note_result,
    now_playing_result, peer_end_result, peer_message_result, play_result, refresh_result,
    schedule_list_result, schedule_result, skip_result, sleep_result, stt_provider_label,
    subscribe_result, subscribe_snapshot, transcript_result, transcript_result_status,
    transcript_source_label, transcript_status_report, unfollow_podcast_result,
    youtube_ingest_metadata, youtube_ingest_result,
};

/// Sentinel UUID used to represent the "Unknown" podcast bucket.
pub(super) const UNKNOWN_PODCAST_ID: &str = "00000000-EEEE-EEEE-EEEE-000000000000";

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_action_tool(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_action_tool",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: Value = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(json!({"error": "Invalid action tool request"})),
            };
            encode(dispatch(&request))
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_action_policy(request_json: *const c_char) -> *mut c_char {
    if request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_action_policy",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: Value = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(json!({"error": "Invalid action policy request"})),
            };
            encode(dispatch(&request))
        },
    )
}

pub(super) fn dispatch(request: &Value) -> Value {
    match string_arg(request, "op").as_str() {
        "success_envelope" => success_envelope(request),
        "error_envelope" => json!({"error": string_arg(request, "message")}),
        "rate_plan" => rate_plan(request),
        "sleep_plan" => sleep_plan(request),
        "seek_plan" => seek_plan(request),
        "play_plan" => play_plan(request),
        "episode_id_plan" => required_arg_plan(request, "episode_id", "Missing or empty 'episode_id'"),
        "podcast_id_plan" => required_arg_plan(request, "podcast_id", "Missing or empty 'podcast_id'"),
        "clip_plan" => clip_plan(request),
        "download_transcribe_plan" => download_transcribe_plan(request),
        "pause_result" => json!({"success": true, "state": "paused"}),
        "rate_result" => json!({
            "success": true,
            "requested_rate": number_arg(request, "requested_rate"),
            "rate": number_arg(request, "rate"),
        }),
        "sleep_result" => sleep_result(request),
        "now_playing_result" => now_playing_result(request),
        "seek_result" => json!({"success": true, "position_seconds": number_arg(request, "position_seconds")}),
        "skip_result" => skip_result(request),
        "episode_mutation_result" => episode_mutation_result(request),
        "transcript_status_report" => transcript_status_report(request),
        "transcript_result_status" => transcript_result_status(request),
        "transcript_source_label" => transcript_source_label(request),
        "stt_provider_label" => stt_provider_label(request),
        "transcript_result" => transcript_result(request),
        "episode_summary_policy" => episode_summary_policy(request),
        "refresh_result" => refresh_result(request),
        "clip_result" => clip_result(request),
        "play_result" => play_result(request),
        "subscribe_plan" => required_arg_plan(request, "feed_url", "Missing or empty 'feed_url'"),
        "subscribe_snapshot" => subscribe_snapshot(request),
        "subscribe_result" => subscribe_result(request),
        "delete_podcast_plan" => required_arg_plan(request, "podcast_id", "Missing or empty 'podcast_id'"),
        "delete_podcast_snapshot" => delete_podcast_snapshot(request),
        "delete_podcast_result" => delete_podcast_result(request),
        "unfollow_podcast_plan" => required_arg_plan(request, "podcast_id", "Missing or empty 'podcast_id'"),
        "unfollow_podcast_result" => unfollow_podcast_result(request),
        "peer_end_plan" => peer_end_plan(request),
        "peer_end_result" => peer_end_result(request),
        "peer_message_plan" => peer_message_plan(request),
        "peer_message_result" => peer_message_result(request),
        "nostr_peer_label" => nostr_peer_label(request),
        "note_plan" => required_arg_plan(request, "text", "Missing note text"),
        "note_result" => note_result(request),
        "memory_plan" => required_arg_plan(request, "content", "Missing memory content"),
        "memory_result" => memory_result(request),
        "schedule_plan" => schedule_plan(request),
        "schedule_result" => schedule_result(request),
        "cancel_schedule_plan" => required_arg_plan(request, "task_id", "'task_id' is required."),
        "cancel_schedule_result" => json!({
            "success": true,
            "task_id": string_arg(request, "task_id"),
            "requested": true,
        }),
        "schedule_list_result" => schedule_list_result(request),
        "tts_publish_plan" => tts_publish_plan(request),
        "category_assignments_plan" => category_assignments_plan(request),
        "category_transcription_disabled_plan" => category_transcription_disabled_plan(request),
        "category_change_plan" => category_change_plan(request),
        "category_change_result" => category_change_result(request),
        "youtube_ingest_plan" => youtube_ingest_plan(request),
        "youtube_ingest_result" => youtube_ingest_result(request),
        "youtube_ingest_metadata" => youtube_ingest_metadata(request),
        "upgrade_thinking_result" => json!({
            "success": true,
            "upgraded": true,
            "model": string_arg(request, "model"),
        }),
        "skill_activation" => skill_activation(request),
        "ask_plan" => ask_plan(request),
        "local_model_selection" => local_model_selection(request),
        "chat_history_upsert" => chat_history_upsert(request),
        "chat_history_normalize" => chat_history_normalize(request),
        "chat_history_wrap_legacy" => chat_history_wrap_legacy(request),
        "agent_activity_record" => agent_activity_record(request),
        "agent_activity_prune" => agent_activity_prune(request),
        "agent_activity_for_batch" => agent_activity_for_batch(request),
        "agent_activity_sorted" => agent_activity_sorted(request),
        "agent_activity_active_count" => agent_activity_active_count(request),
        "agent_activity_undo_batch_ids" => agent_activity_undo_batch_ids(request),
        "agent_activity_mark_undone" => agent_activity_mark_undone(request),
        "agent_run_record" => agent_run_record(request),
        "agent_run_normalize" => agent_run_normalize(request),
        "agent_run_filter" => agent_run_filter(request),
        "pending_friend_register" => pending_friend_register(request),
        "pending_friend_claim" => pending_friend_claim(request),
        "pending_friend_has" => pending_friend_has(request),
        "category_summaries" => category_summaries(request),
        _ => json!({"error": "Unknown action tool operation"}),
    }
}

fn success_envelope(request: &Value) -> Value {
    let mut out = json!({"success": true});
    if let Some(payload) = request.get("payload").and_then(Value::as_object) {
        for (key, value) in payload {
            out[key] = value.clone();
        }
    }
    out
}

fn required_arg_plan(request: &Value, key: &str, message: &str) -> Value {
    let value = string_arg(request, key);
    if value.is_empty() {
        return json!({"error": message});
    }
    json!({ key: value })
}

// ---------------------------------------------------------------------------
// Shared primitive helpers (pub(super) so submodules can import them)
// ---------------------------------------------------------------------------

pub(super) fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    let value = string_arg(args, key);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(super) fn number_arg(args: &Value, key: &str) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or_default()
}

pub(super) fn optional_number_arg(args: &Value, key: &str) -> Option<f64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub(super) fn optional_i64_arg(args: &Value, key: &str) -> Option<i64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub(super) fn usize_arg(args: &Value, key: &str) -> usize {
    args.get(key).and_then(Value::as_u64).unwrap_or_default() as usize
}

pub(super) fn bounded_usize_arg(args: &Value, key: &str, default_value: usize, max_value: usize) -> usize {
    match args.get(key).and_then(Value::as_u64).map(|v| v as usize) {
        Some(0) | None => default_value,
        Some(value) => value.min(max_value),
    }
}

pub(super) fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or_default()
}

pub(super) fn bool_arg_default(args: &Value, key: &str, default_value: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default_value)
}

pub(super) fn insert_optional(out: &mut Value, key: &str, value: Option<String>) {
    if let Some(value) = value {
        out[key] = json!(value);
    }
}

pub(super) fn insert_timestamp(out: &mut Value, key: &str, timestamp: Option<i64>) {
    if let Some(timestamp) = timestamp.and_then(|ts| Utc.timestamp_opt(ts, 0).single()) {
        out[key] = json!(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
}

pub(super) fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

pub(super) fn string_array(args: &Value, key: &str) -> Vec<String> {
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

fn encode(value: Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

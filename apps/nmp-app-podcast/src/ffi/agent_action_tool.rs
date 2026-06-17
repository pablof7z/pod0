//! Rust-owned final envelopes for podcast action tools.
//!
//! Swift executes playback, library, transcript, and clipping capabilities.
//! Rust owns the agent-facing JSON result shape and derived labels/messages.

use std::ffi::{c_char, CStr, CString};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const UNKNOWN_PODCAST_ID: &str = "00000000-EEEE-EEEE-EEEE-000000000000";

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

fn dispatch(request: &Value) -> Value {
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

fn rate_plan(request: &Value) -> Value {
    let Some(rate) = optional_number_arg(request, "rate") else {
        return json!({"error": "Missing or invalid 'rate'"});
    };
    if rate <= 0.0 {
        return json!({"error": "'rate' must be greater than 0"});
    }
    json!({ "rate": rate })
}

fn sleep_plan(request: &Value) -> Value {
    let mode = string_arg(request, "mode").to_lowercase();
    if mode.is_empty() {
        return json!({"error": "Missing or empty 'mode'"});
    }
    if !matches!(mode.as_str(), "off" | "minutes" | "end_of_episode") {
        return json!({"error": "'mode' must be one of: off, minutes, end_of_episode"});
    }
    let is_minutes = mode == "minutes";
    let mut out = json!({ "mode": mode });
    if is_minutes {
        let Some(minutes) = optional_i64_arg(request, "minutes").filter(|m| *m > 0) else {
            return json!({"error": "'minutes' is required when mode is 'minutes'"});
        };
        out["minutes"] = json!(minutes.min(180));
    }
    out
}

fn seek_plan(request: &Value) -> Value {
    let Some(position) = optional_number_arg(request, "position_seconds") else {
        return json!({"error": "Missing or invalid 'position_seconds'"});
    };
    if position < 0.0 {
        return json!({"error": "'position_seconds' must be >= 0"});
    }
    json!({ "position_seconds": position })
}

fn play_plan(request: &Value) -> Value {
    let episode_id = optional_string_arg(request, "episode_id");
    let audio_url = optional_string_arg(request, "audio_url");
    match (episode_id.as_ref(), audio_url.as_ref()) {
        (Some(_), Some(_)) => return json!({"error": "Pass either 'episode_id' OR 'audio_url' - not both."}),
        (None, None) => {
            return json!({"error": "Missing identifier: provide 'episode_id' (library) or 'audio_url' + 'title' (external)."})
        }
        _ => {}
    }
    let start_seconds = optional_number_arg(request, "start_seconds");
    if matches!(start_seconds, Some(s) if s < 0.0) {
        return json!({"error": "'start_seconds' must be >= 0"});
    }
    let end_seconds = optional_number_arg(request, "end_seconds");
    if let (Some(start), Some(end)) = (start_seconds, end_seconds) {
        if end <= start {
            return json!({"error": "'end_seconds' must be greater than 'start_seconds'"});
        }
    }
    if start_seconds.is_none() && matches!(end_seconds, Some(end) if end <= 0.0) {
        return json!({"error": "'end_seconds' must be > 0 when 'start_seconds' is omitted"});
    }
    let queue_position = match string_arg(request, "queue_position").to_lowercase().as_str() {
        "" => "now".to_string(),
        "now" => "now".to_string(),
        "next" => "next".to_string(),
        "end" => "end".to_string(),
        _ => return json!({"error": "'queue_position' must be one of: now, next, end"}),
    };
    let is_external = episode_id.is_none();
    let mut out = json!({
        "source": if is_external { "external" } else { "library" },
        "queue_position": queue_position,
    });
    insert_optional(&mut out, "episode_id", episode_id);
    insert_optional(&mut out, "audio_url", audio_url);
    if let Some(start) = start_seconds {
        out["start_seconds"] = json!(start);
    }
    if let Some(end) = end_seconds {
        out["end_seconds"] = json!(end);
    }
    if is_external {
        let title = string_arg(request, "title");
        if title.is_empty() {
            return json!({"error": "Missing or empty 'title' (required with 'audio_url')."});
        }
        out["title"] = json!(title);
        insert_optional(&mut out, "feed_url", optional_string_arg(request, "feed_url"));
        if let Some(duration) = optional_number_arg(request, "duration_seconds") {
            out["duration_seconds"] = json!(duration);
        }
    }
    out
}

fn clip_plan(request: &Value) -> Value {
    let episode_id = string_arg(request, "episode_id");
    if episode_id.is_empty() {
        return json!({"error": "Missing or empty 'episode_id'"});
    }
    let Some(start_seconds) = optional_number_arg(request, "start_seconds") else {
        return json!({"error": "Missing or invalid 'start_seconds'"});
    };
    let Some(end_seconds) = optional_number_arg(request, "end_seconds") else {
        return json!({"error": "Missing or invalid 'end_seconds'"});
    };
    let mut out = json!({
        "episode_id": episode_id,
        "start_seconds": start_seconds,
        "end_seconds": end_seconds,
    });
    insert_optional(&mut out, "caption", optional_string_arg(request, "caption"));
    insert_optional(&mut out, "transcript_text", optional_string_arg(request, "transcript_text"));
    out
}

fn download_transcribe_plan(request: &Value) -> Value {
    let episode_id = optional_string_arg(request, "episode_id");
    let audio_url = optional_string_arg(request, "audio_url");
    let feed_url = optional_string_arg(request, "feed_url");
    if episode_id.is_none() {
        if let Some(audio_url) = audio_url {
            let Some(feed_url) = feed_url else {
                return json!({
                    "error": "'feed_url' is required when 'episode_id' is not provided. Use subscribe_podcast or search_podcast_directory to get the feed URL first."
                });
            };
            return json!({
                "source": "external",
                "audio_url": audio_url,
                "feed_url": feed_url,
            });
        }
        return json!({
            "error": "Provide 'episode_id' (for subscribed episodes) or 'audio_url' + 'feed_url' (for external episodes)"
        });
    }
    json!({
        "source": "library",
        "episode_id": episode_id,
    })
}

fn sleep_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "mode": string_arg(request, "mode"),
        "label": string_arg(request, "label"),
    });
    if let Some(minutes) = request.get("minutes").and_then(Value::as_i64) {
        out["minutes"] = json!(minutes);
    }
    out
}

fn now_playing_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "is_playing": bool_arg(request, "is_playing"),
        "position_seconds": number_arg(request, "position_seconds"),
        "rate": number_arg(request, "rate"),
    });
    insert_optional(&mut out, "episode_id", optional_string_arg(request, "episode_id"));
    insert_optional(&mut out, "episode_title", optional_string_arg(request, "episode_title"));
    insert_optional(&mut out, "podcast_id", optional_string_arg(request, "podcast_id"));
    insert_optional(&mut out, "podcast_title", optional_string_arg(request, "podcast_title"));
    if let Some(duration) = request.get("duration_seconds").and_then(Value::as_f64) {
        out["duration_seconds"] = json!(duration);
    }
    if optional_string_arg(request, "episode_id").is_none() {
        out["message"] = json!("Nothing is currently loaded.");
    }
    out
}

fn skip_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "new_position_seconds": number_arg(request, "new_position_seconds"),
    });
    if let Some(seconds) = request.get("skipped_seconds").and_then(Value::as_f64) {
        out["skipped_seconds"] = json!(seconds);
    }
    out
}

fn episode_mutation_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "episode_id": string_arg(request, "episode_id"),
        "episode_title": string_arg(request, "episode_title"),
        "state": string_arg(request, "state"),
    });
    insert_optional(&mut out, "podcast_id", optional_string_arg(request, "podcast_id"));
    insert_optional(&mut out, "podcast_title", optional_string_arg(request, "podcast_title"));
    out
}

fn transcript_status_report(request: &Value) -> Value {
    match string_arg(request, "state").as_str() {
        "none" | "ready" => json!({"status": "none"}),
        "queued" => json!({"status": "queued"}),
        "fetching_publisher" => json!({"status": "fetching_publisher"}),
        "transcribing" => json!({"status": "transcribing"}),
        "failed" => {
            let mut out = json!({"status": "failed"});
            insert_optional(&mut out, "message", optional_string_arg(request, "message"));
            out
        }
        _ => json!({"error": "Unknown transcript state"}),
    }
}

fn transcript_result_status(request: &Value) -> Value {
    match string_arg(request, "state").as_str() {
        "ready" => json!({"status": "ready"}),
        "failed" => json!({"status": "failed"}),
        _ => json!({"status": "queued"}),
    }
}

fn transcript_source_label(request: &Value) -> Value {
    let label = match string_arg(request, "source").as_str() {
        "publisher" => "Publisher feed",
        "scribe" => "ElevenLabs Scribe",
        "whisper" => "OpenRouter Whisper",
        "onDevice" => "Apple on-device",
        "assemblyAI" => "AssemblyAI",
        "other" => "Transcription service",
        _ => "Transcription service",
    };
    json!({"label": label})
}

fn stt_provider_label(request: &Value) -> Value {
    let label = match string_arg(request, "provider").as_str() {
        "elevenlabs_scribe" => "ElevenLabs Scribe",
        "assemblyai" => "AssemblyAI",
        "openrouter_whisper" => "OpenRouter Whisper",
        "apple_native" => "Apple on-device",
        _ => "Transcription service",
    };
    json!({"label": label})
}

fn transcript_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "episode_id": string_arg(request, "episode_id"),
        "status": string_arg(request, "status"),
    });
    insert_optional(&mut out, "podcast_id", optional_string_arg(request, "podcast_id"));
    insert_optional(&mut out, "podcast_title", optional_string_arg(request, "podcast_title"));
    insert_optional(&mut out, "source", optional_string_arg(request, "source"));
    insert_optional(&mut out, "message", optional_string_arg(request, "message"));
    out
}

fn episode_summary_policy(request: &Value) -> Value {
    let error = string_arg(request, "error");
    if !error.is_empty() {
        return json!({"outcome": "rejected", "message": error});
    }
    let summary = string_arg(request, "summary");
    if !summary.is_empty() {
        return json!({"outcome": "summary", "summary": summary});
    }
    let publisher_description = string_arg(request, "publisher_description");
    if !publisher_description.is_empty() {
        return json!({"outcome": "summary", "summary": publisher_description});
    }
    json!({"outcome": "unavailable"})
}

fn refresh_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "title": string_arg(request, "title"),
        "episode_count": usize_arg(request, "episode_count"),
        "new_episode_count": usize_arg(request, "new_episode_count"),
    });
    if let Some(timestamp) = request.get("refreshed_at").and_then(Value::as_i64) {
        if let Some(datetime) = Utc.timestamp_opt(timestamp, 0).single() {
            out["refreshed_at"] = json!(datetime.to_rfc3339_opts(SecondsFormat::Secs, true));
        }
    }
    out
}

fn clip_result(request: &Value) -> Value {
    let start = number_arg(request, "start_seconds");
    let end = number_arg(request, "end_seconds");
    let mut out = json!({
        "success": true,
        "clip_id": string_arg(request, "clip_id"),
        "episode_id": string_arg(request, "episode_id"),
        "episode_title": string_arg(request, "episode_title"),
        "start_seconds": start,
        "end_seconds": end,
        "duration_seconds": end - start,
    });
    insert_optional(&mut out, "podcast_id", optional_string_arg(request, "podcast_id"));
    insert_optional(&mut out, "transcript_text", optional_string_arg(request, "transcript_text"));
    insert_optional(&mut out, "caption", optional_string_arg(request, "caption"));
    out
}

fn play_result(request: &Value) -> Value {
    let queue_position = string_arg(request, "queue_position");
    let mut out = json!({
        "success": true,
        "episode_id": string_arg(request, "episode_id"),
        "queue_position": queue_position,
        "started_playing": bool_arg(request, "started_playing"),
    });
    insert_optional(&mut out, "episode_title", optional_string_arg(request, "episode_title"));
    insert_optional(&mut out, "podcast_title", optional_string_arg(request, "podcast_title"));
    insert_optional(&mut out, "audio_url", optional_string_arg(request, "audio_url"));
    insert_optional(&mut out, "title", optional_string_arg(request, "title"));
    insert_optional(&mut out, "feed_url", optional_string_arg(request, "feed_url"));
    if let Some(duration) = request.get("duration_seconds").and_then(Value::as_f64) {
        out["duration_seconds"] = json!(duration);
    }
    if let Some(start) = request.get("start_seconds").and_then(Value::as_f64) {
        out["start_seconds"] = json!(start);
    }
    if let Some(end) = request.get("end_seconds").and_then(Value::as_f64) {
        out["end_seconds"] = json!(end);
    }
    match queue_position.as_str() {
        "next" => {
            out["status"] = json!("queued");
            out["message"] = json!("Added to the front of Up Next.");
        }
        "end" => {
            out["status"] = json!("queued");
            out["message"] = json!("Added to the end of Up Next.");
        }
        _ => {
            out["status"] = json!("playing");
            out["message"] = json!("Playing now.");
        }
    }
    out
}

fn subscribe_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "title": string_arg(request, "title"),
        "feed_url": string_arg(request, "feed_url"),
        "episode_count": usize_arg(request, "episode_count"),
        "already_subscribed": bool_arg(request, "already_subscribed"),
    });
    insert_optional(&mut out, "author", optional_string_arg(request, "author"));
    out
}

fn subscribe_snapshot(request: &Value) -> Value {
    let normalized_feed_url = string_arg(request, "normalized_feed_url");
    let completed = bool_arg(request, "completed");
    if bool_arg(request, "is_already_subscribed") {
        let podcast_id = string_arg(request, "podcast_id");
        if !podcast_id.is_empty() {
            return json!({
                "result": subscribe_result_payload(
                    podcast_id,
                    string_arg(request, "title"),
                    optional_string_arg(request, "author"),
                    optional_string_arg(request, "feed_url").unwrap_or(normalized_feed_url),
                    usize_arg(request, "episode_count"),
                    true,
                )
            });
        }
    }
    if completed {
        let podcast_id = string_arg(request, "podcast_id");
        if !podcast_id.is_empty() {
            return json!({
                "result": subscribe_result_payload(
                    podcast_id,
                    string_arg(request, "title"),
                    optional_string_arg(request, "author"),
                    optional_string_arg(request, "feed_url").unwrap_or(normalized_feed_url),
                    usize_arg(request, "episode_count"),
                    false,
                )
            });
        }
    }
    json!({"should_subscribe": true})
}

fn subscribe_result_payload(
    podcast_id: String,
    title: String,
    author: Option<String>,
    feed_url: String,
    episode_count: usize,
    already_subscribed: bool,
) -> Value {
    let mut out = json!({
        "podcast_id": podcast_id,
        "title": title,
        "feed_url": feed_url,
        "episode_count": episode_count,
        "already_subscribed": already_subscribed,
    });
    insert_optional(&mut out, "author", author);
    out
}

fn delete_podcast_result(request: &Value) -> Value {
    let episodes_deleted = usize_arg(request, "episodes_deleted");
    let was_subscribed = bool_arg(request, "was_subscribed");
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "was_subscribed": was_subscribed,
        "episodes_deleted": episodes_deleted,
        "message": delete_message(was_subscribed, episodes_deleted),
    });
    insert_optional(&mut out, "title", optional_string_arg(request, "title"));
    out
}

fn delete_podcast_snapshot(request: &Value) -> Value {
    let podcast_id = string_arg(request, "podcast_id");
    if podcast_id.eq_ignore_ascii_case(UNKNOWN_PODCAST_ID) {
        return json!({"error": "Cannot delete the Unknown podcast sentinel."});
    }
    let mut out = json!({
        "podcast_id": podcast_id,
        "was_subscribed": bool_arg(request, "was_subscribed"),
        "episodes_deleted": usize_arg(request, "episodes_deleted"),
    });
    insert_optional(&mut out, "title", optional_string_arg(request, "title"));
    json!({"result": out})
}

fn delete_message(was_subscribed: bool, episodes_deleted: usize) -> String {
    let plural = if episodes_deleted == 1 { "" } else { "s" };
    if was_subscribed {
        format!("Unsubscribed and deleted {episodes_deleted} episode{plural}.")
    } else {
        format!("Deleted {episodes_deleted} episode{plural} from a non-subscribed podcast.")
    }
}

fn peer_end_plan(request: &Value) -> Value {
    let root_event_id = string_arg(request, "root_event_id");
    if root_event_id.is_empty() {
        return json!({"error": "end_conversation requires a peer conversation context"});
    }
    let reason = string_arg(request, "reason");
    if reason.is_empty() {
        return json!({"error": "Missing or empty 'reason'"});
    }
    json!({
        "reason": reason,
        "root_event_id": root_event_id,
    })
}

fn peer_end_result(request: &Value) -> Value {
    json!({
        "success": true,
        "no_reply": true,
        "reason": string_arg(request, "reason"),
        "root_event_id": string_arg(request, "root_event_id"),
    })
}

fn peer_message_plan(request: &Value) -> Value {
    let friend_pubkey = string_arg(request, "friend_pubkey");
    if friend_pubkey.is_empty() {
        return json!({"error": "Missing or empty 'friend_pubkey'"});
    }
    let message = string_arg(request, "message");
    if message.is_empty() {
        return json!({"error": "Missing or empty 'message'"});
    }
    json!({
        "friend_pubkey": friend_pubkey,
        "message": message,
    })
}

fn peer_message_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "event_id": string_arg(request, "event_id"),
        "friend_pubkey": string_arg(request, "friend_pubkey"),
        "re_invocation": "Message sent. Once the other agent responds you will be automatically re-invoked in this conversation with their reply.",
    });
    insert_optional(&mut out, "root_event_id", optional_string_arg(request, "root_event_id"));
    out
}

fn nostr_peer_label(request: &Value) -> Value {
    if let Some(display_name) = optional_string_arg(request, "display_name") {
        return json!({"label": display_name});
    }
    let prefix: String = string_arg(request, "pubkey").chars().take(8).collect();
    if prefix.is_empty() {
        json!({"label": "Nostr contact"})
    } else {
        json!({"label": format!("Nostr contact {prefix}")})
    }
}

fn note_result(request: &Value) -> Value {
    let text = string_arg(request, "text");
    json!({
        "success": true,
        "id": string_arg(request, "id"),
        "summary": "Saved note",
        "activity_summary": format!("Saved note \"{}\"", truncate(&text, 80)),
    })
}

fn memory_result(request: &Value) -> Value {
    let content = string_arg(request, "content");
    let message = optional_string_arg(request, "message").unwrap_or_else(|| "Saved memory".to_string());
    json!({
        "success": true,
        "id": string_arg(request, "id"),
        "summary": message,
        "activity_summary": format!("Remembered \"{}\"", truncate(&content, 80)),
    })
}

fn schedule_plan(request: &Value) -> Value {
    let prompt = string_arg(request, "prompt");
    if prompt.is_empty() {
        return json!({"error": "'prompt' is required"});
    }
    let Some(schedule) = requested_schedule(request) else {
        return json!({"error": "Either 'interval_seconds' or 'cadence' is required."});
    };
    let title = optional_string_arg(request, "label").unwrap_or_else(|| truncate(&prompt, 40));
    json!({
        "title": title,
        "prompt": prompt,
        "schedule": schedule,
    })
}

fn schedule_result(request: &Value) -> Value {
    json!({
        "success": true,
        "title": string_arg(request, "title"),
        "schedule": string_arg(request, "schedule"),
        "prompt": string_arg(request, "prompt"),
    })
}

fn schedule_list_result(request: &Value) -> Value {
    let tasks: Vec<Value> = request
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(schedule_task_row)
        .collect();
    let count = tasks.len();
    json!({
        "success": true,
        "tasks": tasks,
        "count": count,
    })
}

fn schedule_task_row(task: &Value) -> Value {
    let mut out = json!({
        "task_id": string_arg(task, "task_id"),
        "title": string_arg(task, "title"),
        "schedule": string_arg(task, "schedule"),
        "status": string_arg(task, "status"),
        "enabled": bool_arg(task, "enabled"),
        "intent_type": optional_string_arg(task, "intent_type").unwrap_or_else(|| "custom".to_string()),
        "intent_label": optional_string_arg(task, "intent_label").unwrap_or_else(|| "Custom task".to_string()),
    });
    insert_optional(&mut out, "intent_detail", optional_string_arg(task, "intent_detail"));
    insert_optional(&mut out, "description", optional_string_arg(task, "description"));
    insert_timestamp(&mut out, "next_run_at", task.get("next_run_at").and_then(Value::as_i64));
    insert_timestamp(&mut out, "last_run_at", task.get("last_run_at").and_then(Value::as_i64));
    out
}

fn tts_publish_plan(request: &Value) -> Value {
    let target_podcast_id = string_arg(request, "target_podcast_id");
    let nostr_enabled = request
        .get("nostr_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if target_podcast_id.is_empty() || !nostr_enabled {
        return json!({"should_publish_to_nostr": false});
    }
    let should_publish = request
        .get("owned_podcasts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|podcast| {
            string_arg(podcast, "podcast_id") == target_podcast_id
                && string_arg(podcast, "visibility") == "public"
        });
    json!({"should_publish_to_nostr": should_publish})
}

fn category_assignments_plan(request: &Value) -> Value {
    let mut assignments: Vec<(String, Vec<String>)> = Vec::new();
    for category in request
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let label = string_arg(category, "name");
        if label.is_empty() {
            continue;
        }
        for podcast_id in string_array(category, "podcast_ids") {
            push_category_label(&mut assignments, podcast_id, label.clone());
        }
    }
    for podcast_id in string_array(request, "followed_podcast_ids") {
        if !assignments.iter().any(|(existing, _)| existing == &podcast_id) {
            assignments.push((podcast_id, Vec::new()));
        }
    }
    json!({
        "assignments": assignments
            .into_iter()
            .map(|(podcast_id, categories)| json!({
                "podcast_id": podcast_id,
                "categories": categories,
            }))
            .collect::<Vec<_>>()
    })
}

fn push_category_label(assignments: &mut Vec<(String, Vec<String>)>, podcast_id: String, label: String) {
    if let Some((_, labels)) = assignments.iter_mut().find(|(existing, _)| existing == &podcast_id) {
        if !labels.iter().any(|existing| existing == &label) {
            labels.push(label);
        }
    } else {
        assignments.push((podcast_id, vec![label]));
    }
}

fn category_transcription_disabled_plan(request: &Value) -> Value {
    let disabled_category_ids: Vec<String> = request
        .get("settings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|setting| !bool_arg(setting, "transcription_enabled"))
        .filter_map(|setting| optional_string_arg(setting, "category_id"))
        .collect();
    let mut podcast_ids: Vec<String> = Vec::new();
    for category in request
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let category_id = string_arg(category, "id");
        if !disabled_category_ids.iter().any(|id| id == &category_id) {
            continue;
        }
        for podcast_id in string_array(category, "podcast_ids") {
            if !podcast_ids.iter().any(|existing| existing == &podcast_id) {
                podcast_ids.push(podcast_id);
            }
        }
    }
    json!({ "podcast_ids": podcast_ids })
}

fn category_change_plan(request: &Value) -> Value {
    let podcast_id = string_arg(request, "podcast_id");
    if podcast_id.is_empty() {
        return json!({"error": "Missing or empty 'podcast_id'"});
    }
    let category_id = optional_string_arg(request, "category_id");
    let category_slug = optional_string_arg(request, "category_slug");
    let category_name = optional_string_arg(request, "category_name");
    if category_id.is_none() && category_slug.is_none() && category_name.is_none() {
        return json!({"error": "Provide one of 'category_id', 'category_slug', or 'category_name'"});
    }
    let mut out = json!({"podcast_id": podcast_id});
    insert_optional(&mut out, "category_id", category_id);
    insert_optional(&mut out, "category_slug", category_slug);
    insert_optional(&mut out, "category_name", category_name);
    out
}

fn category_change_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "title": string_arg(request, "title"),
        "category_id": string_arg(request, "category_id"),
        "category_name": string_arg(request, "category_name"),
        "category_slug": string_arg(request, "category_slug"),
    });
    insert_optional(&mut out, "previous_category_id", optional_string_arg(request, "previous_category_id"));
    insert_optional(&mut out, "previous_category_name", optional_string_arg(request, "previous_category_name"));
    out
}

fn youtube_ingest_result(request: &Value) -> Value {
    let transcribe = bool_arg(request, "transcribe");
    let mut out = json!({
        "success": true,
        "episode_id": string_arg(request, "episode_id"),
        "title": string_arg(request, "title"),
        "author": string_arg(request, "author"),
        "message": if transcribe {
            "YouTube video ingested and transcription queued."
        } else {
            "YouTube video ingested."
        },
    });
    if let Some(duration) = request.get("duration_seconds").and_then(Value::as_i64) {
        out["duration_seconds"] = json!(duration);
    }
    insert_optional(&mut out, "transcript_status", optional_string_arg(request, "transcript_status"));
    out
}

fn youtube_ingest_plan(request: &Value) -> Value {
    let url = string_arg(request, "url");
    if url.is_empty() {
        return json!({"error": "Missing or empty 'url'"});
    }
    let mut out = json!({
        "url": url,
        "transcribe": request.get("transcribe").and_then(Value::as_bool).unwrap_or(true),
    });
    insert_optional(&mut out, "title", optional_string_arg(request, "title"));
    out
}

fn youtube_ingest_metadata(request: &Value) -> Value {
    let title = optional_string_arg(request, "custom_title")
        .or_else(|| optional_string_arg(request, "fallback_title"))
        .unwrap_or_else(|| "YouTube episode".to_string());
    let source_url = string_arg(request, "url");
    json!({
        "title": title,
        "description": if source_url.is_empty() {
            "From YouTube".to_string()
        } else {
            format!("From YouTube: {source_url}")
        },
    })
}

fn skill_activation(request: &Value) -> Value {
    let skill_id = string_arg(request, "skill_id");
    let skills = request
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if skill_id.is_empty() {
        return json!({
            "error": "Missing or empty 'skill_id'",
            "enabled_skills": string_array(request, "enabled_skills"),
        });
    }
    let Some(skill) = skills.iter().find(|skill| string_arg(skill, "skill_id") == skill_id) else {
        let known = skills
            .iter()
            .map(|skill| string_arg(skill, "skill_id"))
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return json!({
            "error": format!("Unknown skill '{skill_id}'. Known skills: {known}."),
            "enabled_skills": string_array(request, "enabled_skills"),
        });
    };
    let mut enabled = string_array(request, "enabled_skills");
    let already_enabled = enabled.iter().any(|id| id == &skill_id);
    if !already_enabled {
        enabled.push(skill_id.clone());
    }
    let mut result = json!({
        "success": true,
        "skill_id": skill_id,
        "display_name": string_arg(skill, "display_name"),
        "already_enabled": already_enabled,
        "tools_unlocked": skill.get("tool_names").cloned().unwrap_or_else(|| json!([])),
        "enabled_skills": enabled,
    });
    if !already_enabled {
        result["manual"] = json!(string_arg(skill, "manual"));
    }
    result
}

fn ask_plan(request: &Value) -> Value {
    let question = string_arg(request, "question");
    if question.is_empty() {
        return json!({"error": "Missing or empty 'question'"});
    }
    let mut out = json!({ "question": question });
    insert_optional(&mut out, "context", optional_string_arg(request, "context"));
    out
}

fn local_model_selection(request: &Value) -> Value {
    for key in [
        "agent_initial_model",
        "agent_thinking_model",
        "categorization_model",
        "chapter_compilation_model",
    ] {
        if let Some(model_id) = local_model_id(&string_arg(request, key)) {
            return json!({"model_id": model_id});
        }
    }
    json!({"model_id": null})
}

fn local_model_id(stored_id: &str) -> Option<String> {
    let trimmed = stored_id.trim();
    let rest = trimmed.strip_prefix("local:")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn chat_history_upsert(request: &Value) -> Value {
    let mut conversations = request
        .get("conversations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(mut conversation) = request.get("conversation").cloned() else {
        return chat_history_normalize_value(conversations);
    };
    let id = string_arg(&conversation, "id");
    let message_count = conversation
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    if message_count == 0 {
        conversations.retain(|existing| string_arg(existing, "id") != id);
    } else {
        cap_conversation_messages(&mut conversation);
        if let Some(index) = conversations.iter().position(|existing| string_arg(existing, "id") == id) {
            conversations[index] = conversation;
        } else {
            conversations.push(conversation);
        }
    }
    chat_history_normalize_value(conversations)
}

fn chat_history_normalize(request: &Value) -> Value {
    let conversations = request
        .get("conversations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    chat_history_normalize_value(conversations)
}

fn chat_history_wrap_legacy(request: &Value) -> Value {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if messages.is_empty() {
        return json!({"conversations": []});
    }
    let updated_at = messages
        .last()
        .map(|message| string_arg(message, "timestamp"))
        .filter(|timestamp| !timestamp.is_empty())
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    let created_at = messages
        .first()
        .map(|message| string_arg(message, "timestamp"))
        .filter(|timestamp| !timestamp.is_empty())
        .unwrap_or_else(|| updated_at.clone());
    let conversation = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "title": "",
        "messages": cap_messages(messages),
        "isUpgraded": bool_arg(request, "is_upgraded"),
        "enabledSkills": [],
        "isScheduledTask": false,
        "createdAt": created_at,
        "updatedAt": updated_at,
    });
    json!({"conversations": [conversation]})
}

fn chat_history_normalize_value(mut conversations: Vec<Value>) -> Value {
    for conversation in &mut conversations {
        cap_conversation_messages(conversation);
    }
    conversations.retain(|conversation| {
        conversation
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| !messages.is_empty())
            .unwrap_or(false)
    });
    conversations.sort_by(|lhs, rhs| string_arg(rhs, "updatedAt").cmp(&string_arg(lhs, "updatedAt")));
    conversations.truncate(50);
    json!({"conversations": conversations})
}

fn cap_conversation_messages(conversation: &mut Value) {
    if let Some(messages) = conversation.get_mut("messages").and_then(Value::as_array_mut) {
        if messages.len() > 100 {
            let start = messages.len() - 100;
            *messages = messages.split_off(start);
        }
    }
}

fn cap_messages(mut messages: Vec<Value>) -> Vec<Value> {
    if messages.len() > 100 {
        let start = messages.len() - 100;
        messages.split_off(start)
    } else {
        messages
    }
}

fn agent_activity_record(request: &Value) -> Value {
    let mut entries = activity_entries(request);
    if let Some(entry) = request.get("entry").cloned() {
        entries.push(entry);
    }
    json!({"entries": trim_agent_activity(entries)})
}

fn agent_activity_prune(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let entries: Vec<Value> = activity_entries(request)
        .into_iter()
        .filter(|entry| {
            cutoff.is_empty() || string_arg(entry, "timestamp").as_str() >= cutoff.as_str()
        })
        .collect();
    json!({"entries": entries})
}

fn agent_activity_for_batch(request: &Value) -> Value {
    let batch_id = string_arg(request, "batch_id");
    let mut entries: Vec<Value> = activity_entries(request)
        .into_iter()
        .filter(|entry| string_arg(entry, "batchID") == batch_id)
        .collect();
    sort_agent_activity_newest_first(&mut entries);
    json!({"entries": entries})
}

fn agent_activity_sorted(request: &Value) -> Value {
    let mut entries = activity_entries(request);
    sort_agent_activity_newest_first(&mut entries);
    json!({"entries": entries})
}

fn agent_activity_active_count(request: &Value) -> Value {
    let count = activity_entries(request)
        .iter()
        .filter(|entry| !bool_arg(entry, "undone"))
        .count();
    json!({"count": count})
}

fn agent_activity_undo_batch_ids(request: &Value) -> Value {
    let batch_id = string_arg(request, "batch_id");
    let ids: Vec<String> = activity_entries(request)
        .iter()
        .filter(|entry| string_arg(entry, "batchID") == batch_id && !bool_arg(entry, "undone"))
        .map(|entry| string_arg(entry, "id"))
        .filter(|id| !id.is_empty())
        .collect();
    json!({"ids": ids})
}

fn agent_activity_mark_undone(request: &Value) -> Value {
    let entry_id = string_arg(request, "entry_id");
    let mut entries = activity_entries(request);
    for entry in &mut entries {
        if string_arg(entry, "id") == entry_id {
            entry["undone"] = json!(true);
            break;
        }
    }
    json!({"entries": entries})
}

fn activity_entries(request: &Value) -> Vec<Value> {
    request
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn trim_agent_activity(entries: Vec<Value>) -> Vec<Value> {
    const MAX_ACTIVITY_ENTRIES: usize = 200;
    if entries.len() <= MAX_ACTIVITY_ENTRIES {
        return entries;
    }
    let excess = entries.len() - MAX_ACTIVITY_ENTRIES;
    let mut removed = 0usize;
    let mut remove = vec![false; entries.len()];
    for (idx, entry) in entries.iter().enumerate() {
        if removed >= excess {
            break;
        }
        if bool_arg(entry, "undone") {
            remove[idx] = true;
            removed += 1;
        }
    }
    for idx in 0..entries.len() {
        if removed >= excess {
            break;
        }
        if !remove[idx] {
            remove[idx] = true;
            removed += 1;
        }
    }
    entries
        .into_iter()
        .enumerate()
        .filter_map(|(idx, entry)| (!remove[idx]).then_some(entry))
        .collect()
}

fn sort_agent_activity_newest_first(entries: &mut [Value]) {
    entries.sort_by(|lhs, rhs| string_arg(rhs, "timestamp").cmp(&string_arg(lhs, "timestamp")));
}

fn agent_run_record(request: &Value) -> Value {
    let mut runs = Vec::new();
    if let Some(run) = request.get("run").cloned() {
        runs.push(run);
    }
    runs.extend(agent_runs(request));
    json!({"runs": cap_agent_runs(runs)})
}

fn agent_run_normalize(request: &Value) -> Value {
    json!({"runs": cap_agent_runs(agent_runs(request))})
}

fn agent_run_filter(request: &Value) -> Value {
    let sources: std::collections::HashSet<String> = string_array(request, "sources")
        .into_iter()
        .collect();
    let outcomes: std::collections::HashSet<String> = string_array(request, "outcomes")
        .into_iter()
        .collect();
    let tool_query = string_arg(request, "tool_name_query").to_ascii_lowercase();
    let runs: Vec<Value> = agent_runs(request)
        .into_iter()
        .filter(|run| {
            (sources.is_empty() || sources.contains(&string_arg(run, "source")))
                && (outcomes.is_empty() || outcomes.contains(&string_arg(run, "finalOutcome")))
                && (tool_query.is_empty() || run_has_tool_query(run, &tool_query))
        })
        .collect();
    json!({"runs": runs})
}

fn agent_runs(request: &Value) -> Vec<Value> {
    request
        .get("runs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn cap_agent_runs(mut runs: Vec<Value>) -> Vec<Value> {
    const MAX_RETAINED_RUNS: usize = 500;
    runs.truncate(MAX_RETAINED_RUNS);
    runs
}

fn run_has_tool_query(run: &Value, query: &str) -> bool {
    run.get("turns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|turn| {
            turn.get("toolDispatches")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .any(|dispatch| {
            string_arg(dispatch, "toolName")
                .to_ascii_lowercase()
                .contains(query)
        })
}

fn pending_friend_register(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let mut messages = pending_friend_messages(request, &cutoff);
    if let Some(message) = request.get("message").cloned() {
        let sent_event_id = string_arg(&message, "sentEventID");
        messages.retain(|existing| string_arg(existing, "sentEventID") != sent_event_id);
        messages.push(message);
    }
    json!({"messages": messages})
}

fn pending_friend_claim(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let root_event_id = string_arg(request, "root_event_id");
    let mut claimed: Option<Value> = None;
    let messages: Vec<Value> = pending_friend_messages(request, &cutoff)
        .into_iter()
        .filter_map(|message| {
            if claimed.is_none() && string_arg(&message, "sentEventID") == root_event_id {
                claimed = Some(message);
                None
            } else {
                Some(message)
            }
        })
        .collect();
    json!({"messages": messages, "claimed": claimed})
}

fn pending_friend_has(request: &Value) -> Value {
    let cutoff = string_arg(request, "cutoff");
    let root_event_id = string_arg(request, "root_event_id");
    let messages = pending_friend_messages(request, &cutoff);
    let exists = messages
        .iter()
        .any(|message| string_arg(message, "sentEventID") == root_event_id);
    json!({"messages": messages, "exists": exists})
}

fn pending_friend_messages(request: &Value, cutoff: &str) -> Vec<Value> {
    request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|message| cutoff.is_empty() || string_arg(message, "sentAt").as_str() >= cutoff)
        .collect()
}

fn category_summaries(request: &Value) -> Value {
    let args = request.get("args").unwrap_or(&Value::Null);
    let include_podcasts = bool_arg_default(args, "include_podcasts", true);
    let limit = bounded_usize_arg(args, "limit", 25, 100);
    let categories_by_id: std::collections::HashMap<String, Value> = request
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|category| (string_arg(category, "id").to_ascii_lowercase(), category.clone()))
        .collect();
    let podcasts_by_id: std::collections::HashMap<String, Value> = request
        .get("podcasts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|podcast| (string_arg(podcast, "podcast_id").to_ascii_lowercase(), podcast.clone()))
        .collect();
    let categories: Vec<Value> = request
        .get("projected_categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(|projected| {
            let category_id = string_arg(projected, "category_id");
            let source = categories_by_id.get(&category_id.to_ascii_lowercase())?;
            let podcast_ids = string_array(projected, "podcast_ids");
            let known_podcast_ids: Vec<&String> = podcast_ids
                .iter()
                .filter(|podcast_id| podcasts_by_id.contains_key(&podcast_id.to_ascii_lowercase()))
                .collect();
            let subscriptions = if include_podcasts {
                known_podcast_ids
                    .iter()
                    .filter_map(|podcast_id| {
                        let podcast = podcasts_by_id.get(&podcast_id.to_ascii_lowercase())?;
                        let mut row = json!({
                            "podcast_id": string_arg(podcast, "podcast_id"),
                            "title": string_arg(podcast, "title"),
                        });
                        insert_optional(
                            &mut row,
                            "author",
                            optional_string_arg(podcast, "author"),
                        );
                        Some(row)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut row = json!({
                "category_id": category_id,
                "name": string_arg(source, "name"),
                "slug": string_arg(source, "slug"),
                "description": string_arg(source, "description"),
                "subscription_count": known_podcast_ids.len(),
                "generated_at": string_arg(source, "generated_at"),
                "subscriptions": subscriptions,
            });
            insert_optional(&mut row, "color_hex", optional_string_arg(source, "color_hex"));
            insert_optional(&mut row, "model", optional_string_arg(source, "model"));
            Some(row)
        })
        .collect();
    json!({"categories": categories})
}

fn string_array(args: &Value, key: &str) -> Vec<String> {
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

fn requested_schedule(request: &Value) -> Option<String> {
    optional_string_arg(request, "cadence").or_else(|| {
        optional_i64_arg(request, "interval_seconds").map(|seconds| format!("every {seconds}s"))
    })
}

fn insert_timestamp(out: &mut Value, key: &str, timestamp: Option<i64>) {
    if let Some(timestamp) = timestamp.and_then(|ts| Utc.timestamp_opt(ts, 0).single()) {
        out[key] = json!(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
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

fn number_arg(args: &Value, key: &str) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn optional_number_arg(args: &Value, key: &str) -> Option<f64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn optional_i64_arg(args: &Value, key: &str) -> Option<i64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn usize_arg(args: &Value, key: &str) -> usize {
    args.get(key).and_then(Value::as_u64).unwrap_or_default() as usize
}

fn bounded_usize_arg(args: &Value, key: &str, default_value: usize, max_value: usize) -> usize {
    match args.get(key).and_then(Value::as_u64).map(|v| v as usize) {
        Some(0) | None => default_value,
        Some(value) => value.min(max_value),
    }
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or_default()
}

fn bool_arg_default(args: &Value, key: &str, default_value: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default_value)
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

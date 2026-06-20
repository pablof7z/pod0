//! Result, snapshot, status, label and payload projection functions for
//! agent action tools.
//!
//! Each function maps a Swift-supplied JSON request to an agent-facing JSON
//! result envelope.

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use super::{
    bool_arg, insert_optional, insert_timestamp, number_arg, optional_string_arg, string_arg,
    truncate, usize_arg, UNKNOWN_PODCAST_ID,
};

pub(super) fn sleep_result(request: &Value) -> Value {
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

pub(super) fn now_playing_result(request: &Value) -> Value {
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

pub(super) fn skip_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "new_position_seconds": number_arg(request, "new_position_seconds"),
    });
    if let Some(seconds) = request.get("skipped_seconds").and_then(Value::as_f64) {
        out["skipped_seconds"] = json!(seconds);
    }
    out
}

pub(super) fn episode_mutation_result(request: &Value) -> Value {
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

pub(super) fn transcript_status_report(request: &Value) -> Value {
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

pub(super) fn transcript_result_status(request: &Value) -> Value {
    match string_arg(request, "state").as_str() {
        "ready" => json!({"status": "ready"}),
        "failed" => json!({"status": "failed"}),
        _ => json!({"status": "queued"}),
    }
}

pub(super) fn transcript_source_label(request: &Value) -> Value {
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

pub(super) fn stt_provider_label(request: &Value) -> Value {
    let label = match string_arg(request, "provider").as_str() {
        "elevenlabs_scribe" => "ElevenLabs Scribe",
        "assemblyai" => "AssemblyAI",
        "openrouter_whisper" => "OpenRouter Whisper",
        "apple_native" => "Apple on-device",
        _ => "Transcription service",
    };
    json!({"label": label})
}

pub(super) fn transcript_result(request: &Value) -> Value {
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

pub(super) fn episode_summary_policy(request: &Value) -> Value {
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

pub(super) fn refresh_result(request: &Value) -> Value {
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

pub(super) fn clip_result(request: &Value) -> Value {
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

pub(super) fn play_result(request: &Value) -> Value {
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

pub(super) fn subscribe_result(request: &Value) -> Value {
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

pub(super) fn subscribe_snapshot(request: &Value) -> Value {
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

pub(super) fn delete_podcast_result(request: &Value) -> Value {
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

pub(super) fn delete_podcast_snapshot(request: &Value) -> Value {
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
        format!("Deleted the podcast and {episodes_deleted} episode{plural}.")
    } else {
        format!("Deleted {episodes_deleted} episode{plural} from a non-subscribed podcast.")
    }
}

pub(super) fn unfollow_podcast_result(request: &Value) -> Value {
    let was_subscribed = bool_arg(request, "was_subscribed");
    let message = if was_subscribed {
        "Unfollowed — your listen history and episodes are kept."
    } else {
        "Podcast was not followed; no changes to follow state."
    };
    let mut out = json!({
        "success": true,
        "podcast_id": string_arg(request, "podcast_id"),
        "was_subscribed": was_subscribed,
        "episodes_kept": true,
        "message": message,
    });
    insert_optional(&mut out, "title", optional_string_arg(request, "title"));
    out
}

pub(super) fn peer_end_result(request: &Value) -> Value {
    json!({
        "success": true,
        "no_reply": true,
        "reason": string_arg(request, "reason"),
        "root_event_id": string_arg(request, "root_event_id"),
    })
}

pub(super) fn peer_message_result(request: &Value) -> Value {
    let mut out = json!({
        "success": true,
        "event_id": string_arg(request, "event_id"),
        "friend_pubkey": string_arg(request, "friend_pubkey"),
        "re_invocation": "Message sent. Once the other agent responds you will be automatically re-invoked in this conversation with their reply.",
    });
    insert_optional(&mut out, "root_event_id", optional_string_arg(request, "root_event_id"));
    out
}

pub(super) fn nostr_peer_label(request: &Value) -> Value {
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

pub(super) fn note_result(request: &Value) -> Value {
    let text = string_arg(request, "text");
    json!({
        "success": true,
        "id": string_arg(request, "id"),
        "summary": "Saved note",
        "activity_summary": format!("Saved note \"{}\"", truncate(&text, 80)),
    })
}

pub(super) fn memory_result(request: &Value) -> Value {
    let content = string_arg(request, "content");
    let message = optional_string_arg(request, "message").unwrap_or_else(|| "Saved memory".to_string());
    json!({
        "success": true,
        "id": string_arg(request, "id"),
        "summary": message,
        "activity_summary": format!("Remembered \"{}\"", truncate(&content, 80)),
    })
}

pub(super) fn schedule_result(request: &Value) -> Value {
    json!({
        "success": true,
        "title": string_arg(request, "title"),
        "schedule": string_arg(request, "schedule"),
        "prompt": string_arg(request, "prompt"),
    })
}

pub(super) fn schedule_list_result(request: &Value) -> Value {
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

pub(super) fn category_change_result(request: &Value) -> Value {
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

pub(super) fn youtube_ingest_result(request: &Value) -> Value {
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

pub(super) fn youtube_ingest_metadata(request: &Value) -> Value {
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

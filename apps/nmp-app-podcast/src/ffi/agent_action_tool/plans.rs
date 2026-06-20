//! Planning functions for agent action tools.
//!
//! Each function validates its inputs and returns a plan JSON value (or an
//! error envelope) that Swift uses to drive a capability execution.

use serde_json::{json, Value};

use super::{
    bool_arg, insert_optional, optional_i64_arg, optional_number_arg, optional_string_arg,
    string_arg, string_array, truncate,
};

pub(super) fn rate_plan(request: &Value) -> Value {
    let Some(rate) = optional_number_arg(request, "rate") else {
        return json!({"error": "Missing or invalid 'rate'"});
    };
    if rate <= 0.0 {
        return json!({"error": "'rate' must be greater than 0"});
    }
    json!({ "rate": rate })
}

pub(super) fn sleep_plan(request: &Value) -> Value {
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

pub(super) fn seek_plan(request: &Value) -> Value {
    let Some(position) = optional_number_arg(request, "position_seconds") else {
        return json!({"error": "Missing or invalid 'position_seconds'"});
    };
    if position < 0.0 {
        return json!({"error": "'position_seconds' must be >= 0"});
    }
    json!({ "position_seconds": position })
}

pub(super) fn play_plan(request: &Value) -> Value {
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

pub(super) fn clip_plan(request: &Value) -> Value {
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

pub(super) fn download_transcribe_plan(request: &Value) -> Value {
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

pub(super) fn peer_end_plan(request: &Value) -> Value {
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

pub(super) fn peer_message_plan(request: &Value) -> Value {
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

pub(super) fn schedule_plan(request: &Value) -> Value {
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

fn requested_schedule(request: &Value) -> Option<String> {
    optional_string_arg(request, "cadence").or_else(|| {
        optional_i64_arg(request, "interval_seconds").map(|seconds| format!("every {seconds}s"))
    })
}

pub(super) fn tts_publish_plan(request: &Value) -> Value {
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

pub(super) fn category_assignments_plan(request: &Value) -> Value {
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

pub(super) fn category_transcription_disabled_plan(request: &Value) -> Value {
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

pub(super) fn category_change_plan(request: &Value) -> Value {
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

pub(super) fn youtube_ingest_plan(request: &Value) -> Value {
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

pub(super) fn ask_plan(request: &Value) -> Value {
    let question = string_arg(request, "question");
    if question.is_empty() {
        return json!({"error": "Missing or empty 'question'"});
    }
    let mut out = json!({ "question": question });
    insert_optional(&mut out, "context", optional_string_arg(request, "context"));
    out
}

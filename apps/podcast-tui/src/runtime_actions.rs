use nmp_app_podcast::ffi::AgentTaskIntent;
use serde_json::json;

use crate::runtime::{AppRuntime, Result};

impl AppRuntime {
    pub fn play_episode(&self, episode_id: &str, _position_secs: f64) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "play", "episode_id": episode_id}),
        )
    }

    pub fn pause(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "pause"}))
    }

    pub fn resume(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "play"}))
    }

    pub fn seek(&self, position_secs: f64) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "seek", "position_secs": position_secs}),
        )
    }

    pub fn skip_forward(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "skip_forward"}))
    }

    pub fn skip_backward(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "skip_backward"}))
    }

    pub fn set_speed(&self, speed: f32) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "set_speed", "speed": speed}),
        )
    }

    pub fn set_volume(&self, volume: f32) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "set_volume", "volume": volume}),
        )
    }

    pub fn stop(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "stop"}))
    }

    pub fn subscribe(&self, feed_url: &str) -> Result<String> {
        self.dispatch_action_value("podcast", &json!({"op": "subscribe", "feed_url": feed_url}))
    }

    pub fn search_itunes(&self, query: &str) -> Result<String> {
        self.dispatch_action_value("podcast", &json!({"op": "search_itunes", "query": query}))
    }

    pub fn download_episode(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "download", "episode_id": episode_id}),
        )
    }

    pub fn pause_download(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "pause_download", "episode_id": episode_id}),
        )
    }

    pub fn resume_download(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "resume_download", "episode_id": episode_id}),
        )
    }

    pub fn cancel_download(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "cancel_download", "episode_id": episode_id}),
        )
    }

    pub fn cancel_all_downloads(&self) -> Result<String> {
        self.dispatch_action_value("podcast.player", &json!({"op": "cancel_all_downloads"}))
    }

    pub fn delete_download(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "delete_download", "episode_id": episode_id}),
        )
    }

    pub fn fetch_transcript(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "fetch_transcript", "episode_id": episode_id}),
        )
    }

    pub fn fetch_chapters(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "fetch_chapters", "episode_id": episode_id}),
        )
    }

    pub fn compile_chapters(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.chapters",
            &json!({"op": "compile", "episode_id": episode_id}),
        )
    }

    pub fn fetch_comments(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "fetch_comments", "episode_id": episode_id}),
        )
    }

    pub fn post_comment(&self, episode_id: &str, content: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "post_comment", "episode_id": episode_id, "content": content}),
        )
    }

    pub fn summarize_episode(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "summarize_episode", "episode_id": episode_id}),
        )
    }

    pub fn reset_progress(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "reset_progress", "episode_id": episode_id}),
        )
    }

    pub fn set_sleep_timer(&self, secs: Option<u64>) -> Result<String> {
        self.dispatch_action_value(
            "podcast.player",
            &json!({"op": "set_sleep_timer", "secs": secs}),
        )
    }

    pub fn add_to_queue(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.queue",
            &json!({"op": "add_last", "episode_id": episode_id}),
        )
    }

    pub fn add_next_to_queue(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.queue",
            &json!({"op": "add_next", "episode_id": episode_id}),
        )
    }

    pub fn remove_from_queue(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.queue",
            &json!({"op": "remove", "episode_id": episode_id}),
        )
    }

    pub fn clear_queue(&self) -> Result<String> {
        self.dispatch_action_value("podcast.queue", &json!({"op": "clear"}))
    }

    pub fn mark_played(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.inbox",
            &json!({"op": "mark_listened", "episode_id": episode_id}),
        )
    }

    pub fn mark_unplayed(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.inbox",
            &json!({"op": "mark_unlistened", "episode_id": episode_id}),
        )
    }

    pub fn star(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "star_episode", "episode_id": episode_id, "starred": true}),
        )
    }

    pub fn unstar(&self, episode_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "star_episode", "episode_id": episode_id, "starred": false}),
        )
    }

    pub fn auto_snip(&self, episode_id: &str, position_secs: f64) -> Result<String> {
        self.dispatch_action_value(
            "podcast.clip",
            &json!({"op": "auto_snip", "episode_id": episode_id, "position_secs": position_secs}),
        )
    }

    pub fn delete_clip(&self, clip_id: &str) -> Result<String> {
        self.dispatch_action_value("podcast.clip", &json!({"op": "delete", "clip_id": clip_id}))
    }

    pub fn send_agent_message(&self, message: &str) -> Result<String> {
        self.dispatch_action_value("podcast.agent", &json!({"op": "send", "message": message}))
    }

    pub fn clear_agent(&self) -> Result<String> {
        self.dispatch_action_value("podcast.agent", &json!({"op": "clear"}))
    }

    pub fn fetch_contacts(&self) -> Result<String> {
        self.dispatch_action_value("podcast", &json!({"op": "fetch_contacts"}))
    }

    pub fn fetch_agent_notes(&self) -> Result<String> {
        self.dispatch_action_value("podcast", &json!({"op": "fetch_agent_notes"}))
    }

    pub fn publish_agent_note(&self, recipient_pubkey_hex: &str, content: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({
                "op": "publish_agent_note",
                "recipient_pubkey_hex": recipient_pubkey_hex,
                "content": content,
                "root_event_id": null,
                "inbound_event_id": null,
                "root_a_tags": [],
            }),
        )
    }

    pub fn create_agent_task(
        &self,
        title: &str,
        schedule: &str,
        intent: &str,
        description: Option<&str>,
    ) -> Result<String> {
        let intent = task_intent_from_input(intent)?;
        self.create_agent_task_from_intent(title, schedule, &intent, description)
    }

    pub fn create_agent_task_from_intent(
        &self,
        title: &str,
        schedule: &str,
        intent: &AgentTaskIntent,
        description: Option<&str>,
    ) -> Result<String> {
        let intent = serde_json::to_value(intent)
            .map_err(|e| format!("failed to encode task intent: {e}"))?;
        self.dispatch_action_value(
            "podcast.tasks",
            &json!({
                "op": "create_from_intent",
                "title": title,
                "description": description,
                "intent": intent,
                "schedule": schedule,
            }),
        )
    }

    pub fn delete_agent_task(&self, task_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.tasks",
            &json!({"op": "delete", "task_id": task_id}),
        )
    }

    pub fn enable_agent_task(&self, task_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.tasks",
            &json!({"op": "enable", "task_id": task_id}),
        )
    }

    pub fn disable_agent_task(&self, task_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.tasks",
            &json!({"op": "disable", "task_id": task_id}),
        )
    }

    pub fn run_agent_task_now(&self, task_id: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.tasks",
            &json!({"op": "run_now", "task_id": task_id}),
        )
    }

    pub fn remember_memory(&self, key: &str, value: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.memory",
            &json!({"op": "remember", "key": key, "value": value, "source": "user"}),
        )
    }

    pub fn forget_memory(&self, key: &str) -> Result<String> {
        self.dispatch_action_value("podcast.memory", &json!({"op": "forget", "key": key}))
    }

    pub fn forget_all_memory(&self) -> Result<String> {
        self.dispatch_action_value("podcast.memory", &json!({"op": "forget_all"}))
    }

    pub fn set_auto_play_next(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_play_next", "enabled": enabled}),
        )
    }

    pub fn set_auto_mark_played_at_end(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_mark_played_at_end", "enabled": enabled}),
        )
    }

    pub fn set_auto_skip_ads(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_skip_ads", "enabled": enabled}),
        )
    }

    pub fn set_auto_delete_downloads_after_played(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_delete_downloads_after_played", "enabled": enabled}),
        )
    }

    pub fn set_notify_on_new_episodes(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_notify_on_new_episodes", "enabled": enabled}),
        )
    }

    pub fn set_auto_ingest_publisher_transcripts(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_ingest_publisher_transcripts", "enabled": enabled}),
        )
    }

    pub fn set_auto_fallback_to_scribe(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_auto_fallback_to_scribe", "enabled": enabled}),
        )
    }

    pub fn set_nostr_enabled(&self, enabled: bool) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_nostr_enabled", "enabled": enabled}),
        )
    }

    pub fn add_relay(&self, url: &str, role: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "add_relay", "url": url, "role": role}),
        )
    }

    pub fn remove_relay(&self, url: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "remove_relay", "url": url}),
        )
    }

    pub fn set_relay_role(&self, url: &str, role: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_relay_role", "url": url, "role": role}),
        )
    }

    pub fn set_default_playback_rate(&self, rate: f64) -> Result<String> {
        self.dispatch_action_value(
            "podcast.settings",
            &json!({"op": "set_default_playback_rate", "rate": rate}),
        )
    }
}

fn task_intent_from_input(input: &str) -> Result<AgentTaskIntent> {
    let input = input.trim();
    match input {
        "inbox_triage" | "triage_inbox" | "triage" => Ok(AgentTaskIntent::InboxTriage),
        "clear_agent" | "agent_clear" | "clear_chat" => Ok(AgentTaskIntent::ClearAgent),
        _ => memory_intent_value(input)
            .or_else(|| prompt_intent_value(input))
            .ok_or_else(|| {
                "unknown task intent; use inbox_triage, clear_agent, memory:key=value, or prompt:<text>"
                    .to_string()
            }),
    }
}

fn memory_intent_value(input: &str) -> Option<AgentTaskIntent> {
    let rest = input.strip_prefix("memory:")?;
    let (key, value) = rest.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some(AgentTaskIntent::RememberMemory {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

fn prompt_intent_value(input: &str) -> Option<AgentTaskIntent> {
    let prompt = input
        .strip_prefix("prompt:")
        .or_else(|| input.strip_prefix("agent_prompt:"))
        .or_else(|| input.strip_prefix("ask_agent:"))?
        .trim();
    if prompt.is_empty() || prompt.starts_with('{') || prompt.starts_with('[') {
        return None;
    }
    Some(AgentTaskIntent::AgentPrompt {
        prompt: prompt.to_owned(),
    })
}

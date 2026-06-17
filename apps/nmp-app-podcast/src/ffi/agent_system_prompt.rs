//! Rust-owned in-app agent system prompt.
//!
//! Swift supplies raw render facts it already has in memory and executes the
//! LLM/tool loop. Rust owns prompt prose, section ordering, caps, truncation,
//! and fallback wording.

use std::ffi::{c_char, CStr, CString};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const TITLE_CHARS: usize = 80;
const NOTE_LIMIT: usize = 20;
const MEMORY_LIMIT: usize = 40;

#[derive(Debug, Deserialize, Default)]
struct AgentSystemPromptRequest {
    #[serde(default)]
    agent_context: Option<PromptAgentContext>,
    #[serde(default)]
    friends: Vec<PromptFriend>,
    #[serde(default)]
    notes: Vec<PromptNote>,
    #[serde(default)]
    memory_facts: Vec<PromptMemoryFact>,
    #[serde(default)]
    skills: Vec<PromptSkill>,
}

#[derive(Debug, Deserialize, Default)]
struct PromptAgentContext {
    #[serde(default)]
    subscriptions: Vec<String>,
    #[serde(default)]
    subscriptions_total: usize,
    #[serde(default)]
    in_progress: Vec<PromptEpisode>,
    #[serde(default)]
    recent_unplayed: Vec<PromptEpisode>,
    #[serde(default)]
    recent_window_days: usize,
}

#[derive(Debug, Deserialize)]
struct PromptEpisode {
    title: String,
    show_title: String,
}

#[derive(Debug, Deserialize)]
struct PromptFriend {
    display_name: String,
    identifier: String,
}

#[derive(Debug, Deserialize)]
struct PromptNote {
    text: String,
    kind: String,
    deleted: bool,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct PromptMemoryFact {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PromptSkill {
    id: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct AgentSystemPromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
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

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_system_prompt(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_system_prompt",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: AgentSystemPromptRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&prompt_error("invalid_request")),
            };
            encode(&AgentSystemPromptResponse {
                error: None,
                system_prompt: Some(build_system_prompt(request)),
            })
        },
    )
}

fn build_system_prompt(request: AgentSystemPromptRequest) -> String {
    let mut sections = Vec::new();
    sections.push(base_instructions());
    sections.push(skills_catalog(&request.skills));

    if let Some(ctx) = request.agent_context {
        push_agent_context(&mut sections, ctx);
    }
    push_friends(&mut sections, &request.friends);
    push_notes(&mut sections, request.notes);
    push_memories(&mut sections, request.memory_facts);

    sections.join("\n\n")
}

fn base_instructions() -> String {
    format!(
        r#"You are a helpful personal assistant embedded in a podcast-player app.
Today is {}.
Help the user surface, recall, and reason about what they have been listening to.
Be concise and action-oriented. For specifics that are not in this prompt (transcripts, episode contents, semantic search), call your tools.

You can play episodes the user is NOT subscribed to. When asked to play a guest appearance, a one-off episode, or anything not in the library:
1. Use `search_podcast_directory` to find the feed URL and audio URL.
2. Use `play_episode(audio_url:, title:, feed_url:)` to start playing immediately. ALWAYS pass feed_url when you have one; the app fetches the show's real artwork and title from it. Only omit feed_url for raw audio links where you genuinely do not know the source podcast.
3. Optionally offer `subscribe_podcast(feed_url)` so the user can follow the show.
For transcripts of external episodes, call `subscribe_podcast` first then `download_and_transcribe(feed_url, audio_url)`.

To browse an unfamiliar show's episodes BEFORE subscribing, call `list_episodes` and pass either the `collection_id` as `podcast_id` or the `feed_url` you got from `search_podcast_directory`. The app captures the show's metadata and episodes without flipping the follow bit. Only call `subscribe_podcast` when the user explicitly says they want to follow the show.

You are running on a fast/cheap model by default. Before answering, judge the request: simple lookups, one-tool answers, short factual replies -> just answer. If the task needs multi-step reasoning, planning, writing code, careful synthesis, or you are not confident you can answer well -> call `upgrade_thinking` first. Subsequent turns will run on a stronger model. Default to NOT upgrading; only upgrade when you are genuinely unsure or the task is clearly complex."#,
        Utc::now().format("%A, %B %-d, %Y at %H:%M UTC")
    )
}

fn skills_catalog(skills: &[PromptSkill]) -> String {
    let lines = skills
        .iter()
        .filter_map(|skill| {
            let id = skill.id.trim();
            let summary = skill.summary.trim();
            if id.is_empty() || summary.is_empty() {
                None
            } else {
                Some(format!("- `{id}` - {summary}"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "## Skills\n{lines}\n\nCall `use_skill(skill_id: \"<id>\")` to load any of these. You will get its full instructions back and unlock its tools for the rest of the conversation. Default to NOT loading a skill unless the user's request matches one. Skill manuals are large; loading a skill you do not need wastes context."
    )
}

fn push_agent_context(sections: &mut Vec<String>, ctx: PromptAgentContext) {
    if !ctx.subscriptions.is_empty() {
        let titles = ctx
            .subscriptions
            .iter()
            .map(|title| format!("- {}", truncate(title)))
            .collect::<Vec<_>>()
            .join("\n");
        let hidden = ctx.subscriptions_total.saturating_sub(ctx.subscriptions.len());
        let suffix = if hidden > 0 {
            format!("\n...and {hidden} more")
        } else {
            String::new()
        };
        sections.push(format!(
            "## Subscriptions ({})\n{}{}",
            ctx.subscriptions_total, titles, suffix
        ));
    }
    if !ctx.in_progress.is_empty() {
        sections.push(format!("## In Progress\n{}", episode_lines(&ctx.in_progress)));
    }
    if !ctx.recent_unplayed.is_empty() {
        sections.push(format!(
            "## Recent (last {} days, unplayed)\n{}",
            ctx.recent_window_days,
            episode_lines(&ctx.recent_unplayed)
        ));
    }
}

fn push_friends(sections: &mut Vec<String>, friends: &[PromptFriend]) {
    if friends.is_empty() {
        return;
    }
    let list = friends
        .iter()
        .filter_map(|friend| {
            let name = friend.display_name.trim();
            let identifier = friend.identifier.trim();
            if name.is_empty() || identifier.is_empty() {
                None
            } else {
                Some(format!("- {name} ({})", short_identifier(identifier)))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !list.is_empty() {
        sections.push(format!("## Friends\n{list}"));
    }
}

fn push_notes(sections: &mut Vec<String>, mut notes: Vec<PromptNote>) {
    notes.retain(|note| !note.deleted && note.kind != "systemEvent");
    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let list = notes
        .into_iter()
        .take(NOTE_LIMIT)
        .filter_map(|note| {
            let text = note.text.trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(format!("- {text}"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !list.is_empty() {
        sections.push(format!("## Notes\n{list}"));
    }
}

fn push_memories(sections: &mut Vec<String>, memories: Vec<PromptMemoryFact>) {
    let list = memories
        .into_iter()
        .filter_map(|fact| {
            let key = fact.key.trim().to_string();
            let value = fact.value.trim().to_string();
            if value.is_empty() {
                None
            } else if key.is_empty() {
                Some(format!("- {value}"))
            } else {
                Some(format!("- {key}: {value}"))
            }
        })
        .take(MEMORY_LIMIT)
        .collect::<Vec<_>>()
        .join("\n");
    if !list.is_empty() {
        sections.push(format!("## What You Know About the User\n{list}"));
    }
}

fn episode_lines(episodes: &[PromptEpisode]) -> String {
    episodes
        .iter()
        .map(|episode| format!("- {} - {}", truncate(&episode.title), episode.show_title))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= TITLE_CHARS {
        s.to_string()
    } else {
        let mut out = s.chars().take(TITLE_CHARS - 3).collect::<String>();
        out.push_str("...");
        out
    }
}

fn short_identifier(identifier: &str) -> String {
    if identifier.chars().count() <= 6 {
        return identifier.to_string();
    }
    identifier.chars().take(6).collect()
}

fn prompt_error(error: &str) -> AgentSystemPromptResponse {
    AgentSystemPromptResponse {
        error: Some(error.to_string()),
        system_prompt: None,
    }
}

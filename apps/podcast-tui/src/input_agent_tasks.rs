use crossterm::event::{KeyCode, KeyEvent};
use nmp_app_podcast::ffi::AgentTaskIntent;

use crate::app::{AppState, Mode, Tab};
use crate::runtime::AppRuntime;

#[derive(Debug)]
struct AgentTaskDraft {
    title: String,
    schedule: String,
    intent: AgentTaskIntent,
    description: Option<String>,
}

pub(super) fn handle_agent_task_input(
    state: &mut AppState,
    runtime: &AppRuntime,
    key: KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.agent_task_input.trim().to_string();
            state.agent_task_input.clear();
            state.mode = Mode::Normal;
            match parse_agent_task_input(&input) {
                Ok(draft) => {
                    match runtime.create_agent_task_from_intent(
                        &draft.title,
                        &draft.schedule,
                        &draft.intent,
                        draft.description.as_deref(),
                    ) {
                        Ok(_) => state.push_toast("task created"),
                        Err(e) => state.status = format!("task error: {e}"),
                    }
                }
                Err(message) => state.status = message,
            }
            state.tab = Tab::Agent;
        }
        KeyCode::Backspace => {
            state.agent_task_input.pop();
        }
        KeyCode::Char(c) => state.agent_task_input.push(c),
        _ => {}
    }
    true
}

fn parse_agent_task_input(input: &str) -> Result<AgentTaskDraft, String> {
    let parts = input.split('|').map(str::trim).collect::<Vec<_>>();
    match parts.as_slice() {
        [schedule, request] => draft_from_request(None, schedule, request, None),
        [schedule, request, description] => {
            draft_from_request(None, schedule, request, optional_text(description))
        }
        [title, schedule, request, description @ ..] => draft_from_request(
            optional_text(title),
            schedule,
            request,
            optional_text(&description.join(" | ")),
        ),
        _ => Err(task_input_hint()),
    }
}

fn draft_from_request(
    explicit_title: Option<&str>,
    schedule: &str,
    request: &str,
    description: Option<&str>,
) -> Result<AgentTaskDraft, String> {
    let schedule = require_text(schedule, task_input_hint)?;
    let (intent, default_title) = parse_task_request(request)?;
    let title = explicit_title.unwrap_or(default_title).to_owned();
    Ok(AgentTaskDraft {
        title,
        schedule: schedule.to_owned(),
        intent,
        description: description.map(str::to_owned),
    })
}

fn parse_task_request(request: &str) -> Result<(AgentTaskIntent, &'static str), String> {
    let normalized = normalize_request(request);
    match normalized.as_str() {
        "triage" | "triage inbox" | "inbox triage" => {
            Ok((AgentTaskIntent::InboxTriage, "Inbox Triage"))
        }
        "clear agent" | "clear chat" | "clear conversation" | "clear agent chat" => {
            Ok((AgentTaskIntent::ClearAgent, "Clear Agent Chat"))
        }
        _ => parse_memory_request(request)
            .map(|intent| (intent, "Remember Memory"))
            .ok_or_else(task_input_hint),
    }
}

fn parse_memory_request(request: &str) -> Option<AgentTaskIntent> {
    let trimmed = request.trim();
    let rest = strip_prefix_ci(trimmed, "remember memory")
        .or_else(|| strip_prefix_ci(trimmed, "remember"))
        .or_else(|| strip_prefix_ci(trimmed, "memory:"))
        .or_else(|| strip_prefix_ci(trimmed, "memory"))?;
    let rest = rest.trim();
    let (key, value) = rest.split_once('=').or_else(|| rest.split_once(':'))?;
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

fn normalize_request(request: &str) -> String {
    request
        .trim()
        .to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &input[prefix.len()..])
}

fn optional_text(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn require_text(text: &str, hint: fn() -> String) -> Result<&str, String> {
    optional_text(text).ok_or_else(hint)
}

fn task_input_hint() -> String {
    "task examples: daily | triage inbox; weekly | remember topic=rust; once | clear agent"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inbox_triage_request() {
        let draft = parse_agent_task_input("daily | triage inbox").unwrap();
        assert_eq!(draft.title, "Inbox Triage");
        assert_eq!(draft.schedule, "daily");
        assert_eq!(draft.intent, AgentTaskIntent::InboxTriage);
    }

    #[test]
    fn parses_memory_request() {
        let draft = parse_agent_task_input("weekly | remember topic=rust | keep fresh").unwrap();
        assert_eq!(draft.title, "Remember Memory");
        assert_eq!(draft.description.as_deref(), Some("keep fresh"));
        assert_eq!(
            draft.intent,
            AgentTaskIntent::RememberMemory {
                key: "topic".to_owned(),
                value: "rust".to_owned()
            }
        );
    }

    #[test]
    fn parses_legacy_explicit_title_form() {
        let draft =
            parse_agent_task_input("Clear Nightly | nightly | clear_agent | old form").unwrap();
        assert_eq!(draft.title, "Clear Nightly");
        assert_eq!(draft.schedule, "nightly");
        assert_eq!(draft.description.as_deref(), Some("old form"));
        assert_eq!(draft.intent, AgentTaskIntent::ClearAgent);
    }

    #[test]
    fn rejects_unknown_requests_with_user_examples() {
        let error = parse_agent_task_input("daily | {\"op\":\"triage\"}").unwrap_err();
        assert!(error.contains("task examples:"));
        assert!(!error.contains("action_body"));
    }
}

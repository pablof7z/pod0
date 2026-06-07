#[path = "input_downloads.rs"]
mod downloads;
#[path = "input_provider_catalog.rs"]
mod provider_catalog;
#[path = "input_settings.rs"]
mod settings;
#[path = "input_tabs.rs"]
mod tabs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, Mode, Tab};
use crate::runtime::AppRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFlow {
    Continue,
    Quit,
}

pub fn handle_key(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> InputFlow {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return InputFlow::Quit;
    }

    if handle_mode_key(state, runtime, key) {
        return InputFlow::Continue;
    }

    if key.code == KeyCode::Char('q') {
        return InputFlow::Quit;
    }

    if key.code == KeyCode::Char('?') {
        state.toggle_help();
        return InputFlow::Continue;
    }

    if key.code == KeyCode::Esc && state.close_help() {
        return InputFlow::Continue;
    }

    match key.code {
        KeyCode::Tab => state.next_tab(),
        KeyCode::BackTab => state.previous_tab(),
        KeyCode::Char('n') if !matches!(state.tab, Tab::Agent | Tab::Settings) => {
            state.mode = Mode::SubscribeInput;
            state.subscribe_input.clear();
            state.status = "enter feed URL to subscribe".to_string();
            return InputFlow::Continue;
        }
        KeyCode::Char('/') => {
            state.mode = Mode::SearchInput;
            state.search_input.clear();
            state.status = "enter search query".to_string();
            return InputFlow::Continue;
        }
        _ => {}
    }

    match state.tab {
        Tab::Library => tabs::handle_library_keys(state, runtime, key),
        Tab::Queue => tabs::handle_queue_keys(state, runtime, key),
        Tab::Inbox => tabs::handle_inbox_keys(state, runtime, key),
        Tab::Search => tabs::handle_search_keys(state, runtime, key),
        Tab::Downloads => downloads::handle_downloads_keys(state, runtime, key),
        Tab::Bookmarks => tabs::handle_bookmark_keys(state, runtime, key),
        Tab::Clips => tabs::handle_clips_keys(state, runtime, key),
        Tab::Agent => tabs::handle_agent_keys(state, runtime, key),
        Tab::Wiki => tabs::handle_wiki_keys(state, runtime, key),
        Tab::Social => tabs::handle_social_keys(state, runtime, key),
        Tab::Settings => settings::handle_settings_keys(state, runtime, key),
    }

    InputFlow::Continue
}

fn handle_mode_key(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match state.mode {
        Mode::SearchInput => handle_search_input(state, runtime, key),
        Mode::SubscribeInput => handle_subscribe_input(state, runtime, key),
        Mode::RelayInput => settings::handle_relay_input(state, runtime, key),
        Mode::SettingsInput => settings::handle_settings_input(state, runtime, key),
        Mode::ProviderCatalog => {
            provider_catalog::handle_provider_catalog_keys(state, runtime, key)
        }
        Mode::AgentInput => handle_agent_input(state, runtime, key),
        Mode::AgentMemoryInput => handle_agent_memory_input(state, runtime, key),
        Mode::AgentTaskInput => handle_agent_task_input(state, runtime, key),
        Mode::AgentNoteInput => handle_agent_note_input(state, runtime, key),
        Mode::EpisodeCommentInput => handle_episode_comment_input(state, runtime, key),
        Mode::EpisodeDetail { .. } => handle_episode_detail_key(state, runtime, key),
        Mode::Normal => false,
    }
}

fn handle_search_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let query = state.search_input.trim().to_string();
            state.mode = Mode::Normal;
            state.tab = Tab::Search;
            if !query.is_empty() {
                match runtime.search_itunes(&query) {
                    Ok(_) => state.status = format!("searching iTunes for: {query}"),
                    Err(e) => state.status = format!("search error: {e}"),
                }
            }
        }
        KeyCode::Backspace => {
            state.search_input.pop();
        }
        KeyCode::Char(c) => state.search_input.push(c),
        _ => {}
    }
    true
}

fn handle_subscribe_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let url = state.subscribe_input.trim().to_string();
            state.mode = Mode::Normal;
            if !url.is_empty() {
                match runtime.subscribe(&url) {
                    Ok(_) => state.status = format!("subscribing to: {url}"),
                    Err(e) => state.status = format!("subscribe error: {e}"),
                }
            }
        }
        KeyCode::Backspace => {
            state.subscribe_input.pop();
        }
        KeyCode::Char(c) => state.subscribe_input.push(c),
        _ => {}
    }
    true
}

fn handle_agent_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let message = state.agent_input.trim().to_string();
            state.agent_input.clear();
            state.mode = Mode::Normal;
            state.tab = Tab::Agent;
            if !message.is_empty() {
                match runtime.send_agent_message(&message) {
                    Ok(_) => state.status = "sent agent message".to_string(),
                    Err(e) => state.status = format!("agent send error: {e}"),
                }
            }
        }
        KeyCode::Backspace => {
            state.agent_input.pop();
        }
        KeyCode::Char(c) => state.agent_input.push(c),
        _ => {}
    }
    true
}

fn handle_agent_memory_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.agent_memory_input.trim().to_string();
            state.agent_memory_input.clear();
            state.mode = Mode::Normal;
            match input.split_once('=') {
                Some((key, value)) if !key.trim().is_empty() => {
                    match runtime.remember_memory(key.trim(), value.trim()) {
                        Ok(_) => state.push_toast("memory saved"),
                        Err(e) => state.status = format!("memory error: {e}"),
                    }
                }
                _ => state.status = "memory format: key=value".to_string(),
            }
        }
        KeyCode::Backspace => {
            state.agent_memory_input.pop();
        }
        KeyCode::Char(c) => state.agent_memory_input.push(c),
        _ => {}
    }
    true
}

fn handle_agent_task_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.agent_task_input.trim().to_string();
            state.agent_task_input.clear();
            state.mode = Mode::Normal;
            match parse_task_input(&input) {
                Some((title, schedule, intent, description)) => {
                    match runtime.create_agent_task(
                        title,
                        schedule,
                        intent,
                        description.filter(|text| !text.is_empty()),
                    ) {
                        Ok(_) => state.push_toast("task created"),
                        Err(e) => state.status = format!("task error: {e}"),
                    }
                }
                None => {
                    state.status =
                        "task format: title | schedule | intent | description".to_string();
                }
            }
        }
        KeyCode::Backspace => {
            state.agent_task_input.pop();
        }
        KeyCode::Char(c) => state.agent_task_input.push(c),
        _ => {}
    }
    true
}

fn handle_agent_note_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.agent_note_input.trim().to_string();
            state.agent_note_input.clear();
            state.mode = Mode::Normal;
            match input.split_once(' ') {
                Some((recipient, content))
                    if !recipient.trim().is_empty() && !content.trim().is_empty() =>
                {
                    match runtime.publish_agent_note(recipient.trim(), content.trim()) {
                        Ok(_) => state.push_toast("agent note published"),
                        Err(e) => state.status = format!("agent note error: {e}"),
                    }
                }
                _ => state.status = "note format: recipient_pubkey_hex message".to_string(),
            }
        }
        KeyCode::Backspace => {
            state.agent_note_input.pop();
        }
        KeyCode::Char(c) => state.agent_note_input.push(c),
        _ => {}
    }
    true
}

fn handle_episode_comment_input(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let content = state.episode_comment_input.trim().to_string();
            state.episode_comment_input.clear();
            state.mode = Mode::Normal;
            if content.is_empty() {
                return true;
            }
            let Some(episode_id) = state.selected_episode_id() else {
                return true;
            };
            state.comments_episode_id = Some(episode_id.clone());
            match runtime.post_comment(&episode_id, &content) {
                Ok(_) => state.push_toast("comment posted"),
                Err(e) => state.status = format!("comment error: {e}"),
            }
        }
        KeyCode::Backspace => {
            state.episode_comment_input.pop();
        }
        KeyCode::Char(c) => state.episode_comment_input.push(c),
        _ => {}
    }
    true
}

fn handle_episode_detail_key(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => state.close_episode_detail(),
        KeyCode::Char('j') | KeyCode::Down => state.episode_detail_scroll_down(),
        KeyCode::Char('k') | KeyCode::Up => state.episode_detail_scroll_up(),
        KeyCode::Char('g') | KeyCode::Home => state.episode_detail_scroll_top(),
        KeyCode::Char('p') => play_selected_episode(state, runtime),
        KeyCode::Char('d') => download_selected_episode(state, runtime),
        KeyCode::Char('D') => delete_selected_episode_download(state, runtime),
        KeyCode::Char('s') => star_selected_episode(state, runtime),
        KeyCode::Char('S') => unstar_selected_episode(state, runtime),
        KeyCode::Char('a') => queue_selected_episode(state, runtime, false),
        KeyCode::Char('A') => queue_selected_episode(state, runtime, true),
        KeyCode::Char('c') => clip_selected_episode(state, runtime),
        KeyCode::Char('t') => fetch_selected_episode_transcript(state, runtime),
        KeyCode::Char('H') => fetch_selected_episode_chapters(state, runtime),
        KeyCode::Char('u') => compile_selected_episode_chapters(state, runtime),
        KeyCode::Char('m') => summarize_selected_episode(state, runtime),
        KeyCode::Char('f') => fetch_selected_episode_comments(state, runtime),
        KeyCode::Char('C') => begin_episode_comment(state),
        KeyCode::Char('R') => reset_selected_episode_progress(state, runtime),
        KeyCode::Char('z') => arm_sleep_timer(state, runtime, 15 * 60),
        KeyCode::Char('Z') => arm_sleep_timer(state, runtime, 30 * 60),
        KeyCode::Char('x') => cancel_sleep_timer(state, runtime),
        _ => {}
    }
    true
}

fn play_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        let _ = runtime.play_episode(&id, 0.0);
        state.status = format!("playing {id}");
    }
}

fn download_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        let _ = runtime.download_episode(&id);
        state.push_toast("download queued");
    }
}

fn delete_selected_episode_download(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        delete_download_for_episode_id(state, runtime, &id);
    }
}

fn delete_download_for_episode_id(state: &mut AppState, runtime: &AppRuntime, episode_id: &str) {
    match runtime.delete_download(episode_id) {
        Ok(_) => state.push_toast("download deleted"),
        Err(e) => state.status = format!("delete download error: {e}"),
    }
}

fn star_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        let _ = runtime.star(&id);
        state.push_toast("starred");
    }
}

fn unstar_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        let _ = runtime.unstar(&id);
        state.push_toast("unstarred");
    }
}

fn queue_selected_episode(state: &mut AppState, runtime: &AppRuntime, next: bool) {
    if let Some(id) = state.selected_episode_id() {
        let result = if next {
            runtime.add_next_to_queue(&id)
        } else {
            runtime.add_to_queue(&id)
        };
        if result.is_ok() {
            state.push_toast(if next { "added next" } else { "added to queue" });
        }
    }
}

fn clip_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some((id, position)) = state.selected_episode_clip_target() {
        match runtime.auto_snip(&id, position) {
            Ok(_) => state.push_toast("clip saved"),
            Err(e) => state.status = format!("clip error: {e}"),
        }
    }
}

fn fetch_selected_episode_transcript(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        match runtime.fetch_transcript(&id) {
            Ok(_) => state.push_toast("fetching transcript"),
            Err(e) => state.status = format!("transcript error: {e}"),
        }
    }
}

fn fetch_selected_episode_chapters(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        match runtime.fetch_chapters(&id) {
            Ok(_) => state.push_toast("fetching chapters"),
            Err(e) => state.status = format!("chapters error: {e}"),
        }
    }
}

fn compile_selected_episode_chapters(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        match runtime.compile_chapters(&id) {
            Ok(_) => state.push_toast("compiling chapters"),
            Err(e) => state.status = format!("chapter compile error: {e}"),
        }
    }
}

fn summarize_selected_episode(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        match runtime.summarize_episode(&id) {
            Ok(_) => state.push_toast("summarizing episode"),
            Err(e) => state.status = format!("summary error: {e}"),
        }
    }
}

fn fetch_selected_episode_comments(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        state.comments_episode_id = Some(id.clone());
        match runtime.fetch_comments(&id) {
            Ok(_) => state.push_toast("fetching comments"),
            Err(e) => state.status = format!("comments error: {e}"),
        }
    }
}

fn begin_episode_comment(state: &mut AppState) {
    state.mode = Mode::EpisodeCommentInput;
    state.episode_comment_input.clear();
    state.status = "enter comment".to_string();
}

fn reset_selected_episode_progress(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(id) = state.selected_episode_id() {
        match runtime.reset_progress(&id) {
            Ok(_) => state.push_toast("progress reset"),
            Err(e) => state.status = format!("reset error: {e}"),
        }
    }
}

fn arm_sleep_timer(state: &mut AppState, runtime: &AppRuntime, secs: u64) {
    match runtime.set_sleep_timer(Some(secs)) {
        Ok(_) => state.push_toast("sleep timer armed"),
        Err(e) => state.status = format!("sleep timer error: {e}"),
    }
}

fn cancel_sleep_timer(state: &mut AppState, runtime: &AppRuntime) {
    match runtime.set_sleep_timer(None) {
        Ok(_) => state.push_toast("sleep timer cancelled"),
        Err(e) => state.status = format!("sleep timer error: {e}"),
    }
}

type TaskInputParts<'a> = (&'a str, &'a str, &'a str, Option<&'a str>);

fn parse_task_input(input: &str) -> Option<TaskInputParts<'_>> {
    let parts = input.split('|').map(str::trim).collect::<Vec<_>>();
    let [title, schedule, intent, rest @ ..] = parts.as_slice() else {
        return None;
    };
    if title.is_empty() || schedule.is_empty() || intent.is_empty() {
        return None;
    }
    Some((*title, *schedule, *intent, rest.first().copied()))
}

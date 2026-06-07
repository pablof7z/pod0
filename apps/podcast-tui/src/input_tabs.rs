use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{AgentSection, AppState, Mode, Pane};
use crate::runtime::AppRuntime;

pub(super) fn handle_library_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            state.focus(Pane::Library);
            state.status = "focus: library".to_string();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            state.focus(Pane::Episodes);
            state.status = "focus: episodes".to_string();
        }
        KeyCode::Char('j') | KeyCode::Down => match state.focused {
            Pane::Library => state.next_podcast(),
            Pane::Episodes => state.next_episode(),
            Pane::Player => {}
        },
        KeyCode::Char('k') | KeyCode::Up => match state.focused {
            Pane::Library => state.previous_podcast(),
            Pane::Episodes => state.previous_episode(),
            Pane::Player => {}
        },
        KeyCode::Char('g') | KeyCode::Home => jump_library_selection(state, false),
        KeyCode::Char('G') | KeyCode::End => jump_library_selection(state, true),
        KeyCode::Char(' ') => toggle_pause(state, runtime),
        KeyCode::Char('d') => super::download_selected_episode(state, runtime),
        KeyCode::Char('D') => super::delete_selected_episode_download(state, runtime),
        KeyCode::Char('s') => super::star_selected_episode(state, runtime),
        KeyCode::Char('S') => super::unstar_selected_episode(state, runtime),
        KeyCode::Char('a') => super::queue_selected_episode(state, runtime, false),
        KeyCode::Char('A') => super::queue_selected_episode(state, runtime, true),
        KeyCode::Char('c') => super::clip_selected_episode(state, runtime),
        KeyCode::Char('p') => super::play_selected_episode(state, runtime),
        KeyCode::Enter if state.focused == Pane::Episodes => state.open_episode_detail(),
        _ => {}
    }
}

pub(super) fn handle_queue_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_queue_item(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_queue_item(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_queue = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_queue = state.queue.len().saturating_sub(1);
        }
        KeyCode::Char('p') | KeyCode::Enter => {
            if let Some(id) = state.selected_queue_episode_id() {
                let _ = runtime.play_episode(&id, 0.0);
                state.status = format!("playing queued episode {id}");
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_queue_episode_id() {
                let _ = runtime.remove_from_queue(&id);
                state.push_toast("removed from queue");
            }
        }
        KeyCode::Char('D') => {
            if let Some(id) = state.selected_queue_episode_id() {
                super::delete_download_for_episode_id(state, runtime, &id);
            }
        }
        KeyCode::Char('x') => {
            let _ = runtime.clear_queue();
            state.push_toast("queue cleared");
        }
        KeyCode::Char(' ') => toggle_pause(state, runtime),
        _ => {}
    }
}

pub(super) fn handle_inbox_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_inbox_item(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_inbox_item(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_inbox = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_inbox = state.inbox.len().saturating_sub(1);
        }
        KeyCode::Char('p') | KeyCode::Enter => {
            if let Some(id) = state.selected_inbox_episode_id() {
                let _ = runtime.play_episode(&id, 0.0);
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_inbox_episode_id() {
                let _ = runtime.download_episode(&id);
                state.push_toast("download queued");
            }
        }
        KeyCode::Char('D') => {
            if let Some(id) = state.selected_inbox_episode_id() {
                super::delete_download_for_episode_id(state, runtime, &id);
            }
        }
        KeyCode::Char('m') => {
            if let Some(id) = state.selected_inbox_episode_id() {
                let _ = runtime.mark_played(&id);
                state.push_toast("marked played");
            }
        }
        KeyCode::Char(' ') => toggle_pause(state, runtime),
        _ => {}
    }
}

pub(super) fn handle_search_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_search_result(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_search_result(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_search = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_search = state.search_results.len().saturating_sub(1);
        }
        KeyCode::Char('s') | KeyCode::Enter => {
            if let Some(url) = state.selected_search_feed_url() {
                match runtime.subscribe(&url) {
                    Ok(_) => state.status = format!("subscribing to: {url}"),
                    Err(e) => state.status = format!("subscribe error: {e}"),
                }
            } else {
                state.status = "no feed_url for selected result".to_string();
            }
        }
        _ => {}
    }
}

pub(super) fn handle_bookmark_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_bookmark(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_bookmark(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_bookmark = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_bookmark = state.bookmarks.len().saturating_sub(1);
        }
        KeyCode::Char('p') | KeyCode::Enter => {
            if let Some(id) = state.selected_bookmark_episode_id() {
                let _ = runtime.play_episode(&id, 0.0);
            }
        }
        KeyCode::Char('S') | KeyCode::Char('d') => {
            if let Some(id) = state.selected_bookmark_episode_id() {
                let _ = runtime.unstar(&id);
                state.push_toast("bookmark removed");
            }
        }
        KeyCode::Char('D') => {
            if let Some(id) = state.selected_bookmark_episode_id() {
                super::delete_download_for_episode_id(state, runtime, &id);
            }
        }
        KeyCode::Char('a') => {
            if let Some(id) = state.selected_bookmark_episode_id() {
                let _ = runtime.add_to_queue(&id);
                state.push_toast("added to queue");
            }
        }
        KeyCode::Char('A') => {
            if let Some(id) = state.selected_bookmark_episode_id() {
                let _ = runtime.add_next_to_queue(&id);
                state.push_toast("added next");
            }
        }
        _ => {}
    }
}

pub(super) fn handle_clips_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_clip(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_clip(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_clip = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_clip = state.clips.len().saturating_sub(1);
        }
        KeyCode::Char('p') | KeyCode::Enter => {
            if let Some((episode_id, start_secs)) = state.selected_clip_play_target() {
                let _ = runtime.play_episode(&episode_id, 0.0);
                let _ = runtime.seek(start_secs);
                state.status = format!("playing clip at {:.0}s", start_secs);
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_clip_id() {
                let _ = runtime.delete_clip(&id);
                state.push_toast("clip deleted");
            }
        }
        KeyCode::Char('c') => {
            if let Some((episode_id, position)) = state.now_playing_clip_target() {
                let _ = runtime.auto_snip(&episode_id, position);
                state.push_toast("clip saved");
            }
        }
        _ => {}
    }
}

pub(super) fn handle_agent_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => state.previous_agent_section(),
        KeyCode::Char('l') | KeyCode::Right => state.next_agent_section(),
        KeyCode::Char('j') | KeyCode::Down => state.next_agent_row(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_agent_row(),
        KeyCode::Char('g') | KeyCode::Home => state.jump_agent_top(),
        KeyCode::Char('G') | KeyCode::End => state.jump_agent_bottom(),
        KeyCode::Enter | KeyCode::Char('i') => begin_agent_primary_input(state),
        KeyCode::Char('n') => begin_agent_new_input(state),
        KeyCode::Char('p') => play_selected_agent_pick(state, runtime),
        KeyCode::Char('a') => queue_selected_agent_pick(state, runtime, false),
        KeyCode::Char('A') => queue_selected_agent_pick(state, runtime, true),
        KeyCode::Char('r') => run_or_refresh_agent_section(state, runtime),
        KeyCode::Char('e') => toggle_selected_agent_task(state, runtime),
        KeyCode::Char('d') => delete_selected_agent_row(state, runtime),
        KeyCode::Char('x') => clear_agent_section(state, runtime),
        KeyCode::Char('c') if state.agent_section == AgentSection::Chat => {
            let _ = runtime.clear_agent();
            state.push_toast("agent conversation cleared");
        }
        _ => {}
    }
}

fn begin_agent_primary_input(state: &mut AppState) {
    match state.agent_section {
        AgentSection::Chat => {
            state.mode = Mode::AgentInput;
            state.agent_input.clear();
            state.status = "enter agent message".to_string();
        }
        AgentSection::Memory => {
            state.mode = Mode::AgentMemoryInput;
            state.agent_memory_input.clear();
            state.status = "memory format: key=value".to_string();
        }
        AgentSection::Tasks => {
            state.mode = Mode::AgentTaskInput;
            state.agent_task_input.clear();
            state.status =
                "task examples: daily | triage inbox; weekly | remember topic=rust".to_string();
        }
        AgentSection::Notes => {
            state.mode = Mode::AgentNoteInput;
            state.agent_note_input.clear();
            state.status = "note format: recipient_pubkey_hex message".to_string();
        }
        AgentSection::Picks => {}
    }
}

fn begin_agent_new_input(state: &mut AppState) {
    match state.agent_section {
        AgentSection::Tasks | AgentSection::Memory | AgentSection::Notes => {
            begin_agent_primary_input(state)
        }
        AgentSection::Chat | AgentSection::Picks => {}
    }
}

fn play_selected_agent_pick(state: &mut AppState, runtime: &AppRuntime) {
    if state.agent_section != AgentSection::Picks {
        return;
    }
    if let Some(episode_id) = state.selected_agent_pick_episode_id() {
        let _ = runtime.play_episode(&episode_id, 0.0);
        state.status = format!("playing agent pick {episode_id}");
    }
}

fn queue_selected_agent_pick(state: &mut AppState, runtime: &AppRuntime, next: bool) {
    if state.agent_section != AgentSection::Picks {
        return;
    }
    if let Some(episode_id) = state.selected_agent_pick_episode_id() {
        let result = if next {
            runtime.add_next_to_queue(&episode_id)
        } else {
            runtime.add_to_queue(&episode_id)
        };
        if result.is_ok() {
            state.push_toast(if next {
                "pick added next"
            } else {
                "pick queued"
            });
        }
    }
}

fn run_or_refresh_agent_section(state: &mut AppState, runtime: &AppRuntime) {
    match state.agent_section {
        AgentSection::Tasks => {
            if let Some(task_id) = state.selected_agent_task_id() {
                if state.selected_agent_task_enabled() == Some(false) {
                    state.status = "task run error: task disabled".to_string();
                    return;
                }
                match runtime.run_agent_task_now(&task_id) {
                    Ok(_) => state.push_toast("task dispatched"),
                    Err(e) => state.status = format!("task run error: {e}"),
                }
            }
        }
        AgentSection::Notes => {
            let _ = runtime.fetch_agent_notes();
            state.push_toast("refreshing agent notes");
        }
        AgentSection::Chat | AgentSection::Picks | AgentSection::Memory => {}
    }
}

fn toggle_selected_agent_task(state: &mut AppState, runtime: &AppRuntime) {
    if state.agent_section != AgentSection::Tasks {
        return;
    }
    let Some(task_id) = state.selected_agent_task_id() else {
        return;
    };
    let enabled = state.selected_agent_task_enabled().unwrap_or(false);
    let result = if enabled {
        runtime.disable_agent_task(&task_id)
    } else {
        runtime.enable_agent_task(&task_id)
    };
    match result {
        Ok(_) => state.push_toast(if enabled {
            "task disabled"
        } else {
            "task enabled"
        }),
        Err(e) => state.status = format!("task toggle error: {e}"),
    }
}

fn delete_selected_agent_row(state: &mut AppState, runtime: &AppRuntime) {
    match state.agent_section {
        AgentSection::Tasks => {
            if let Some(task_id) = state.selected_agent_task_id() {
                match runtime.delete_agent_task(&task_id) {
                    Ok(_) => state.push_toast("task deleted"),
                    Err(e) => state.status = format!("task delete error: {e}"),
                }
            }
        }
        AgentSection::Memory => {
            if let Some(key) = state.selected_memory_key() {
                match runtime.forget_memory(&key) {
                    Ok(_) => state.push_toast("memory forgotten"),
                    Err(e) => state.status = format!("memory delete error: {e}"),
                }
            }
        }
        AgentSection::Chat | AgentSection::Picks | AgentSection::Notes => {}
    }
}

fn clear_agent_section(state: &mut AppState, runtime: &AppRuntime) {
    match state.agent_section {
        AgentSection::Chat => {
            let _ = runtime.clear_agent();
            state.push_toast("agent conversation cleared");
        }
        AgentSection::Memory => match runtime.forget_all_memory() {
            Ok(_) => state.push_toast("all memory forgotten"),
            Err(e) => state.status = format!("memory clear error: {e}"),
        },
        AgentSection::Tasks | AgentSection::Picks | AgentSection::Notes => {}
    }
}

pub(super) fn handle_wiki_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_wiki(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_wiki(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_wiki = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_wiki = state.wiki_articles.len().saturating_sub(1);
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_wiki_id() {
                let _ = runtime.delete_wiki_article(&id);
                state.push_toast("wiki article deleted");
            }
        }
        _ => {}
    }
}

pub(super) fn handle_social_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('r') => {
            let _ = runtime.fetch_contacts();
            state.push_toast("refreshing contacts");
        }
        KeyCode::Char('n') => {
            let _ = runtime.fetch_agent_notes();
            state.push_toast("refreshing agent notes");
        }
        _ => {}
    }
}

fn jump_library_selection(state: &mut AppState, bottom: bool) {
    match state.focused {
        Pane::Library => {
            state.selected_podcast = if bottom {
                state.library.len().saturating_sub(1)
            } else {
                0
            };
            state.selected_episode = 0;
            state.rebuild_selected_episodes();
        }
        Pane::Episodes => {
            state.selected_episode = if bottom {
                state.episodes.len().saturating_sub(1)
            } else {
                0
            };
        }
        Pane::Player => {}
    }
}

fn toggle_pause(state: &mut AppState, runtime: &AppRuntime) {
    if let Some(ref np) = state.now_playing {
        if np.is_playing {
            let _ = runtime.pause();
        } else {
            let _ = runtime.resume();
        }
    }
}

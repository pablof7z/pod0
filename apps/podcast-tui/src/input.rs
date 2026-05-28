use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, Mode, Pane, Tab};
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

    match state.mode {
        Mode::SearchInput => {
            if key.code == KeyCode::Esc {
                state.mode = Mode::Normal;
                return InputFlow::Continue;
            }
            if key.code == KeyCode::Enter {
                let query = state.search_input.clone();
                state.mode = Mode::Normal;
                match runtime.search_itunes(&query) {
                    Ok(_) => state.status = format!("searching iTunes for: {query}"),
                    Err(e) => state.status = format!("search error: {e}"),
                }
                state.tab = Tab::Search;
                return InputFlow::Continue;
            }
            if key.code == KeyCode::Backspace {
                state.search_input.pop();
                return InputFlow::Continue;
            }
            if let KeyCode::Char(c) = key.code {
                state.search_input.push(c);
            }
            return InputFlow::Continue;
        }
        Mode::SubscribeInput => {
            if key.code == KeyCode::Esc {
                state.mode = Mode::Normal;
                return InputFlow::Continue;
            }
            if key.code == KeyCode::Enter {
                let url = state.subscribe_input.clone();
                state.mode = Mode::Normal;
                match runtime.subscribe(&url) {
                    Ok(_) => state.status = format!("subscribing to: {url}"),
                    Err(e) => state.status = format!("subscribe error: {e}"),
                }
                return InputFlow::Continue;
            }
            if key.code == KeyCode::Backspace {
                state.subscribe_input.pop();
                return InputFlow::Continue;
            }
            if let KeyCode::Char(c) = key.code {
                state.subscribe_input.push(c);
            }
            return InputFlow::Continue;
        }
        Mode::Normal => {}
        Mode::EpisodeDetail { .. } => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                    state.close_episode_detail();
                }
                KeyCode::Char('j') | KeyCode::Down => state.episode_detail_scroll_down(),
                KeyCode::Char('k') | KeyCode::Up => state.episode_detail_scroll_up(),
                KeyCode::Char('g') | KeyCode::Home => state.episode_detail_scroll_top(),
                KeyCode::Char('p') => {
                    if let Some(id) = state.selected_episode_id() {
                        let _ = runtime.play_episode(&id, 0.0);
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(id) = state.selected_episode_id() {
                        let _ = runtime.download_episode(&id);
                        state.push_toast("download queued");
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(id) = state.selected_episode_id() {
                        let _ = runtime.star(&id);
                        state.push_toast("starred");
                    }
                }
                KeyCode::Char('a') => {
                    if let Some(id) = state.selected_episode_id() {
                        let _ = runtime.add_to_queue(&id);
                        state.push_toast("added to queue");
                    }
                }
                _ => {}
            }
            return InputFlow::Continue;
        }
    }

    if key.code == KeyCode::Char('q') {
        return InputFlow::Quit;
    }

    if key.code == KeyCode::Char('?') {
        state.toggle_help();
        return InputFlow::Continue;
    }

    if key.code == KeyCode::Esc {
        if state.close_help() {
            return InputFlow::Continue;
        }
    }

    match key.code {
        KeyCode::Tab => state.next_tab(),
        KeyCode::BackTab => state.previous_tab(),
        KeyCode::Char('n') => {
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
        Tab::Library => handle_library_keys(state, runtime, key),
        Tab::Queue => handle_queue_keys(state, runtime, key),
        Tab::Inbox => handle_inbox_keys(state, runtime, key),
        Tab::Search => handle_search_keys(state, runtime, key),
        Tab::Settings => handle_settings_keys(state, runtime, key),
    }

    InputFlow::Continue
}

fn handle_library_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            state.focus(Pane::Library);
            state.status = "focus: library".to_string();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            state.focus(Pane::Episodes);
            state.status = "focus: episodes".to_string();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match state.focused {
                Pane::Library => state.next_podcast(),
                Pane::Episodes => state.next_episode(),
                _ => {}
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match state.focused {
                Pane::Library => state.previous_podcast(),
                Pane::Episodes => state.previous_episode(),
                _ => {}
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            match state.focused {
                Pane::Library => state.selected_podcast = 0,
                Pane::Episodes => state.selected_episode = 0,
                _ => {}
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            match state.focused {
                Pane::Library => {
                    state.selected_podcast = state.library.len().saturating_sub(1);
                }
                Pane::Episodes => {
                    state.selected_episode = state.episodes.len().saturating_sub(1);
                }
                _ => {}
            }
        }
        KeyCode::Char(' ') => {
            if let Some(ref np) = state.now_playing {
                if np.is_playing {
                    let _ = runtime.pause();
                } else {
                    let _ = runtime.resume();
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.download_episode(&id_str);
                state.push_toast("download queued");
            }
        }
        KeyCode::Char('s') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.star(&id_str);
                state.push_toast("starred");
            }
        }
        KeyCode::Char('S') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.unstar(&id_str);
                state.push_toast("unstarred");
            }
        }
        KeyCode::Char('a') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.add_to_queue(&id_str);
                state.push_toast("added to queue");
            }
        }
        KeyCode::Char('p') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.play_episode(&id_str, 0.0);
                state.status = format!("playing {id_str}");
            }
        }
        KeyCode::Enter => {
            if state.focused == Pane::Episodes {
                state.open_episode_detail();
            }
        }
        _ => {}
    }
}

fn handle_queue_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_queue_item(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_queue_item(),
        KeyCode::Char('d') => {
            if let Some(id) = state.selected_episode_id() {
                let id_str = id;
                let _ = runtime.remove_from_queue(&id_str);
            }
        }
        KeyCode::Char(' ') => {
            if let Some(ref np) = state.now_playing {
                if np.is_playing {
                    let _ = runtime.pause();
                } else {
                    let _ = runtime.resume();
                }
            }
        }
        _ => {}
    }
}

fn handle_search_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
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
                    Ok(_) => {
                        state.push_toast(&format!("subscribing to {}", url));
                        state.status = format!("subscribing to: {url}");
                    }
                    Err(e) => state.status = format!("subscribe error: {e}"),
                }
            } else {
                state.status = "no feed_url for selected result".to_string();
            }
        }
        _ => {}
    }
}

fn handle_settings_keys(_state: &mut AppState, _runtime: &AppRuntime, _key: KeyEvent) {
    // placeholder
}

fn handle_inbox_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_inbox_item(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_inbox_item(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_inbox = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_inbox = state.inbox.len().saturating_sub(1);
        }
        KeyCode::Char('p') => {
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
        KeyCode::Char(' ') => {
            if let Some(ref np) = state.now_playing {
                if np.is_playing {
                    let _ = runtime.pause();
                } else {
                    let _ = runtime.resume();
                }
            }
        }
        _ => {}
    }
}

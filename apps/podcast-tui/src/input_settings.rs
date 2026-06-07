use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::provider_catalog;
use crate::app::{AppState, Mode, SettingsSection, Tab};
use crate::provider_settings_catalog::{ProviderSettingItem, PROVIDER_SETTINGS_ITEMS};
use crate::runtime::AppRuntime;
use crate::settings_catalog::SETTINGS_ITEMS;
use crate::settings_state::next_relay_role;

pub(super) fn handle_settings_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => state.previous_settings_section(),
        KeyCode::Char('l') | KeyCode::Right => state.next_settings_section(),
        _ => match state.settings_section {
            SettingsSection::General => handle_general_settings_keys(state, runtime, key),
            SettingsSection::Providers => handle_provider_settings_keys(state, runtime, key),
            SettingsSection::Relays => handle_relay_settings_keys(state, runtime, key),
        },
    }
}

fn handle_general_settings_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_setting(SETTINGS_ITEMS.len()),
        KeyCode::Char('k') | KeyCode::Up => state.previous_setting(),
        KeyCode::Char('g') | KeyCode::Home => state.selected_setting = 0,
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_setting = SETTINGS_ITEMS.len().saturating_sub(1);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(item) = SETTINGS_ITEMS.get(state.selected_setting) {
                match item.activate(state, runtime) {
                    Ok(_) => state.push_toast("setting updated"),
                    Err(e) => state.status = format!("settings error: {e}"),
                }
            }
        }
        _ => {}
    }
}

fn handle_provider_settings_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.next_provider_setting(PROVIDER_SETTINGS_ITEMS.len());
        }
        KeyCode::Char('k') | KeyCode::Up => state.previous_provider_setting(),
        KeyCode::Char('g') | KeyCode::Home => state.jump_provider_setting_top(),
        KeyCode::Char('G') | KeyCode::End => {
            state.jump_provider_setting_bottom(PROVIDER_SETTINGS_ITEMS.len());
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if selected_provider_item(state).is_some_and(|item| item.is_model_setting()) {
                if let Some(item) = selected_provider_item(state) {
                    provider_catalog::open_provider_catalog(state, runtime, item);
                }
            } else {
                activate_provider_setting(state, runtime);
            }
        }
        KeyCode::Char('b') => {
            if let Some(item) = selected_provider_item(state) {
                provider_catalog::open_provider_catalog(state, runtime, item);
            }
        }
        KeyCode::Char('e') => {
            activate_provider_setting(state, runtime);
        }
        _ => {}
    }
}

fn handle_relay_settings_keys(state: &mut AppState, runtime: &AppRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => state.next_relay(),
        KeyCode::Char('k') | KeyCode::Up => state.previous_relay(),
        KeyCode::Char('g') | KeyCode::Home => state.jump_relay_top(),
        KeyCode::Char('G') | KeyCode::End => state.jump_relay_bottom(),
        KeyCode::Char('n') | KeyCode::Char('a') => begin_relay_input(state),
        KeyCode::Char('d') => remove_selected_relay(state, runtime),
        KeyCode::Char('r') | KeyCode::Enter | KeyCode::Char(' ') => {
            cycle_selected_relay_role(state, runtime)
        }
        _ => {}
    }
}

pub(super) fn handle_settings_input(
    state: &mut AppState,
    runtime: &AppRuntime,
    key: KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.settings_input.trim().to_owned();
            state.settings_input.clear();
            state.mode = Mode::Normal;
            state.tab = Tab::Settings;
            state.settings_section = SettingsSection::Providers;
            let Some(item) = PROVIDER_SETTINGS_ITEMS.get(state.selected_provider_setting) else {
                return true;
            };
            match item.apply_input(&input, runtime) {
                Ok(message) => state.push_toast(&message),
                Err(e) => state.status = format!("provider setting error: {e}"),
            }
        }
        KeyCode::Backspace => {
            state.settings_input.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.settings_input.clear();
        }
        KeyCode::Char(c) => state.settings_input.push(c),
        _ => {}
    }
    true
}

pub(super) fn handle_relay_input(
    state: &mut AppState,
    runtime: &AppRuntime,
    key: KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Enter => {
            let input = state.relay_input.trim().to_string();
            state.relay_input.clear();
            state.mode = Mode::Normal;
            state.tab = Tab::Settings;
            state.settings_section = SettingsSection::Relays;
            if input.is_empty() {
                return true;
            }
            let mut parts = input.split_whitespace();
            let url = parts.next().unwrap_or_default();
            let role = parts.next().unwrap_or("both");
            match runtime.add_relay(url, role) {
                Ok(_) => state.push_toast("relay added"),
                Err(e) => state.status = format!("relay add error: {e}"),
            }
        }
        KeyCode::Backspace => {
            state.relay_input.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.relay_input.clear();
        }
        KeyCode::Char(c) => state.relay_input.push(c),
        _ => {}
    }
    true
}

fn begin_relay_input(state: &mut AppState) {
    state.mode = Mode::RelayInput;
    state.relay_input.clear();
    state.status = "relay format: wss://relay.example [role]".to_string();
}

fn activate_provider_setting(state: &mut AppState, runtime: &AppRuntime) {
    let Some(item) = selected_provider_item(state) else {
        return;
    };
    if item.is_immediate() {
        let settings = state.settings.clone();
        match item.activate_immediate(&settings, runtime) {
            Ok(message) => state.push_toast(&message),
            Err(e) => state.status = format!("provider setting error: {e}"),
        }
        return;
    }
    state.mode = Mode::SettingsInput;
    state.settings_input = item.input_value(&state.settings);
    state.status = item.input_hint().to_owned();
}

fn selected_provider_item(state: &AppState) -> Option<ProviderSettingItem> {
    PROVIDER_SETTINGS_ITEMS
        .get(state.selected_provider_setting)
        .copied()
}

fn remove_selected_relay(state: &mut AppState, runtime: &AppRuntime) {
    let Some(url) = state.selected_relay_url() else {
        return;
    };
    match runtime.remove_relay(&url) {
        Ok(_) => state.push_toast("relay removed"),
        Err(e) => state.status = format!("relay remove error: {e}"),
    }
}

fn cycle_selected_relay_role(state: &mut AppState, runtime: &AppRuntime) {
    let Some(url) = state.selected_relay_url() else {
        return;
    };
    let role = state
        .selected_relay_role()
        .map(|role| next_relay_role(&role).to_string())
        .unwrap_or_else(|| "both".to_string());
    match runtime.set_relay_role(&url, &role) {
        Ok(_) => state.push_toast("relay role updated"),
        Err(e) => state.status = format!("relay role error: {e}"),
    }
}

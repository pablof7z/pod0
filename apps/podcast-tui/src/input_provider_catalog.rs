use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, Mode, SettingsSection, Tab};
use crate::provider_model_catalog::visible_provider_models;
use crate::provider_settings_catalog::{load_env_credentials, ProviderSettingItem};
use crate::runtime::AppRuntime;

pub(super) fn handle_provider_catalog_keys(
    state: &mut AppState,
    runtime: &AppRuntime,
    key: KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Esc => close_provider_catalog(state),
        KeyCode::Enter => apply_selected_provider_model(state, runtime),
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            advance_catalog_selection(state);
        }
        KeyCode::Down => advance_catalog_selection(state),
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.previous_provider_catalog_model();
        }
        KeyCode::Up => state.previous_provider_catalog_model(),
        KeyCode::Home => state.jump_provider_catalog_top(),
        KeyCode::End => jump_catalog_bottom(state),
        KeyCode::Backspace => {
            state.provider_catalog_query.pop();
            state.selected_provider_catalog_model = 0;
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.provider_catalog_query.clear();
            state.selected_provider_catalog_model = 0;
        }
        KeyCode::Char(c) => {
            state.provider_catalog_query.push(c);
            state.selected_provider_catalog_model = 0;
        }
        _ => {}
    }
    true
}

pub(super) fn open_provider_catalog(
    state: &mut AppState,
    runtime: &AppRuntime,
    item: ProviderSettingItem,
) {
    if !item.is_catalog_browsable() {
        state.status = "selected provider row is not catalog-browsable".to_owned();
        return;
    }
    let _ = load_env_credentials(runtime);
    let result = if item == ProviderSettingItem::ElevenLabsVoice {
        runtime.elevenlabs_voice_catalog().map(|voices| {
            voices
                .into_iter()
                .map(|voice| voice.into_catalog_model())
                .collect()
        })
    } else {
        runtime.provider_model_catalog()
    };
    match result {
        Ok(models) => {
            state.provider_catalog_models = models;
            state.provider_catalog_target = Some(item);
            state.provider_catalog_query.clear();
            state.selected_provider_catalog_model = 0;
            state.mode = Mode::ProviderCatalog;
            state.tab = Tab::Settings;
            state.settings_section = SettingsSection::Providers;
            state.status = format!(
                "provider catalog loaded: {} items",
                state.provider_catalog_models.len()
            );
        }
        Err(e) => state.status = format!("provider catalog error: {e}"),
    }
}

fn advance_catalog_selection(state: &mut AppState) {
    let count = visible_catalog_count(state);
    state.next_provider_catalog_model(count);
}

fn jump_catalog_bottom(state: &mut AppState) {
    let count = visible_catalog_count(state);
    state.jump_provider_catalog_bottom(count);
}

fn visible_catalog_count(state: &AppState) -> usize {
    visible_provider_models(
        &state.provider_catalog_models,
        state.provider_catalog_target,
        &state.provider_catalog_query,
    )
    .len()
}

fn close_provider_catalog(state: &mut AppState) {
    state.mode = Mode::Normal;
    state.tab = Tab::Settings;
    state.settings_section = SettingsSection::Providers;
    state.provider_catalog_target = None;
}

fn apply_selected_provider_model(state: &mut AppState, runtime: &AppRuntime) {
    let visible = visible_provider_models(
        &state.provider_catalog_models,
        state.provider_catalog_target,
        &state.provider_catalog_query,
    );
    let Some((_, model)) = visible.get(state.selected_provider_catalog_model).copied() else {
        state.status = "no provider model selected".to_owned();
        return;
    };
    let Some(target) = state.provider_catalog_target else {
        state.status = "provider model target missing".to_owned();
        return;
    };
    match target.apply_catalog_selection(model.selection_id(), model.display_name(), runtime) {
        Ok(message) => {
            state.push_toast(&message);
            close_provider_catalog(state);
        }
        Err(e) => state.status = format!("provider model error: {e}"),
    }
}

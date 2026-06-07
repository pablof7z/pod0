use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, SettingsSection};
use crate::provider_settings_catalog::PROVIDER_SETTINGS_ITEMS;
use crate::settings_catalog::SETTINGS_ITEMS;
use crate::ui::{format, theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let right = Layout::vertical([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(columns[1]);

    render_interactive_settings(frame, columns[0], state);
    render_provider_editor(frame, right[0], state);
    render_relay_editor(frame, right[1], state);
}

fn render_interactive_settings(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = theme::panel(
        format!("Settings ({})", state.settings_section.label()),
        state.settings_section == SettingsSection::General,
    );

    let items = SETTINGS_ITEMS
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let selected = index == state.selected_setting;
            let base = if selected {
                theme::selected()
            } else {
                theme::text()
            };
            ListItem::new(Line::from(vec![
                theme::selected_prefix(selected, state.motion_tick),
                Span::styled(item.label(), base),
                Span::styled(format!("  {}", item.value(state)), theme::muted()),
            ]))
        })
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items).block(block), area);
}

fn render_provider_editor(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = theme::panel(
        format!("Models And Providers ({})", state.settings_section.label()),
        state.settings_section == SettingsSection::Providers,
    );

    let visible_rows = area.height.saturating_sub(2).max(1) as usize;
    let start = visible_start(
        state.selected_provider_setting,
        PROVIDER_SETTINGS_ITEMS.len(),
        visible_rows,
    );

    let items = PROVIDER_SETTINGS_ITEMS
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
        .map(|(index, item)| {
            let selected = state.settings_section == SettingsSection::Providers
                && index == state.selected_provider_setting;
            let base = if selected {
                theme::selected()
            } else {
                theme::text()
            };
            ListItem::new(Line::from(vec![
                theme::selected_prefix(selected, state.motion_tick),
                Span::styled(item.label(), base),
                Span::styled(
                    format!(
                        "  {}",
                        item.value(
                            &state.settings,
                            &state.speech_model_catalog,
                            &state.local_model_catalog,
                        )
                    ),
                    theme::muted(),
                ),
            ]))
        })
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items).block(block), area);
}

fn visible_start(selected: usize, len: usize, visible_rows: usize) -> usize {
    if len <= visible_rows || selected < visible_rows {
        0
    } else {
        (selected + 1).saturating_sub(visible_rows)
    }
}

fn render_relay_editor(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = theme::panel(
        format!("App Relays ({})", state.configured_relays.len()),
        state.settings_section == SettingsSection::Relays,
    );

    if state.configured_relays.is_empty() {
        let text = Paragraph::new("No configured relays. Press n to add wss://relay [role].")
            .style(theme::muted())
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items = state
        .configured_relays
        .iter()
        .enumerate()
        .map(|(index, relay)| {
            let selected =
                state.settings_section == SettingsSection::Relays && index == state.selected_relay;
            let base = if selected {
                theme::selected()
            } else {
                theme::text()
            };
            ListItem::new(Line::from(vec![
                theme::selected_prefix(selected, state.motion_tick),
                Span::styled(format::short_id(&relay.url), base),
                Span::styled(format!("  {}", relay.role), theme::muted()),
            ]))
        })
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items).block(block), area);
}

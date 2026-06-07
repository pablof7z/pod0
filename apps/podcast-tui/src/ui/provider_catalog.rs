use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::provider_model_catalog::{visible_provider_models, ProviderCatalogModel};
use crate::ui::{format, theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup_area = centered_rect(86, 82, area);
    frame.render_widget(Clear, popup_area);

    let target = state
        .provider_catalog_target
        .map(|item| item.label())
        .unwrap_or("model");
    let visible = visible_provider_models(
        &state.provider_catalog_models,
        state.provider_catalog_target,
        &state.provider_catalog_query,
    );
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(popup_area);

    let header = Paragraph::new(format!(
        "Target: {target}   Query: {}   Matches: {}/{}",
        state.provider_catalog_query,
        visible.len(),
        state.provider_catalog_models.len()
    ))
    .style(theme::text())
    .block(theme::panel("Provider Model Catalog", true));
    frame.render_widget(header, rows[0]);

    if visible.is_empty() {
        let empty = Paragraph::new("No provider models match the current search.")
            .style(theme::muted())
            .block(theme::panel("Models", true));
        frame.render_widget(empty, rows[1]);
    } else {
        let visible_rows = rows[1].height.saturating_sub(2).max(1) as usize;
        let start = visible_start(
            state.selected_provider_catalog_model,
            visible.len(),
            visible_rows,
        );
        let items = visible
            .iter()
            .skip(start)
            .take(visible_rows)
            .enumerate()
            .map(|(offset, (_, model))| {
                let index = start + offset;
                model_row(model, index == state.selected_provider_catalog_model)
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(items).block(theme::panel("Models", true)),
            rows[1],
        );
    }

    let footer_text = selected_description(state).unwrap_or_else(|| target.to_owned());
    let footer = Paragraph::new(footer_text).style(theme::muted());
    frame.render_widget(footer, rows[2]);
}

fn model_row(model: &ProviderCatalogModel, selected: bool) -> ListItem<'static> {
    let base = if selected {
        theme::selected()
    } else {
        theme::text()
    };
    let provider = format::short_id(&model.provider_name);
    ListItem::new(Line::from(vec![
        Span::styled(model.display_name().to_owned(), base),
        Span::styled(format!("  {provider}"), theme::accent()),
        Span::styled(format!("  {}", model.id), theme::muted()),
        Span::styled(format!("  {}", model.compact_price()), theme::muted()),
        Span::styled(format!("  {}", model.compact_flags()), theme::muted()),
    ]))
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn visible_start(selected: usize, len: usize, visible_rows: usize) -> usize {
    if len <= visible_rows || selected < visible_rows {
        0
    } else {
        (selected + 1).saturating_sub(visible_rows)
    }
}

fn selected_description(state: &AppState) -> Option<String> {
    let visible = visible_provider_models(
        &state.provider_catalog_models,
        state.provider_catalog_target,
        &state.provider_catalog_query,
    );
    visible
        .get(state.selected_provider_catalog_model)
        .and_then(|(_, model)| model.model_description.clone())
}

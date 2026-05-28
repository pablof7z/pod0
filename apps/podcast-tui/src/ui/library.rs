use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, Pane};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let is_focused = state.focused == Pane::Library;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Library ({}) ", state.library.len()));

    if state.library.is_empty() {
        let text = Paragraph::new("No podcasts. Press 'n' to subscribe.").block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .library
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let is_selected = i == state.selected_podcast;
            let title_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![Span::styled(&podcast.title, title_style)];
            if podcast.unplayed_count > 0 {
                spans.push(Span::styled(
                    format!(" ({})", podcast.unplayed_count),
                    Style::default().fg(Color::Yellow),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

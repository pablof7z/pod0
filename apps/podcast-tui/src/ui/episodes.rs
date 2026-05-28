use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::{AppState, Pane};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let is_focused = state.focused == Pane::Episodes;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Episodes ({}) ", state.episodes.len()));

    if state.episodes.is_empty() {
        let text = ratatui::widgets::Paragraph::new("Select a podcast to see episodes.").block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .episodes
        .iter()
        .enumerate()
        .map(|(i, ep)| {
            let is_selected = i == state.selected_episode;
            let base_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = Vec::new();
            if ep.played {
                spans.push(Span::styled("  ", base_style));
            } else {
                spans.push(Span::styled("● ", Style::default().fg(Color::Cyan)));
            }

            spans.push(Span::styled(&ep.title, base_style));

            let mut meta_parts = Vec::new();
            if let Some(dur) = ep.duration_secs {
                meta_parts.push(format_duration(dur));
            }
            if let Some(pos) = ep.playback_position_secs {
                if pos > 0.0 {
                    meta_parts.push(format!("{:.0}%", (pos / ep.duration_secs.unwrap_or(1.0)) * 100.0));
                }
            }
            if !meta_parts.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", meta_parts.join(" | ")),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

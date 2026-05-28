use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Inbox ({}) ", state.inbox.len()));

    if state.inbox.is_empty() {
        let text = Paragraph::new("Inbox is empty. Listen to episodes to triage them.").block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .inbox
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let is_selected = i == state.selected_inbox;
            let base_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![Span::styled(&row.episode_title,
                base_style,
            )];

            let mut meta_parts = Vec::new();
            if !row.podcast_title.is_empty() {
                meta_parts.push(row.podcast_title.clone());
            }
            if let Some(dur) = row.duration_secs {
                meta_parts.push(format_duration(dur));
            }
            if row.priority_score > 0.0 {
                meta_parts.push(format!("{:.0}%", row.priority_score * 100.0));
            }
            if !row.ai_categories.is_empty() {
                meta_parts.push(row.ai_categories.join(", "));
            }
            if !meta_parts.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", meta_parts.join(" | ")),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            if let Some(ref reason) = row.priority_reason {
                spans.push(Span::styled(
                    format!("  — {reason}"),
                    Style::default().fg(Color::Yellow),
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

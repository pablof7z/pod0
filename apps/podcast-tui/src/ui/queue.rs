use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Up Next ({}) ", state.queue.len()));

    if state.queue.is_empty() {
        let text = Paragraph::new("Queue is empty. Press 'a' on an episode to add.").block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .queue
        .iter()
        .enumerate()
        .map(|(i, ep)| {
            let mut spans = vec![
                Span::styled(format!("{}. ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(&ep.title, Style::default().fg(Color::White)),
            ];
            if let Some(dur) = ep.duration_secs {
                spans.push(Span::styled(
                    format!("  {}", format_duration(dur)),
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

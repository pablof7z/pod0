use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Settings ");

    let lines = vec![
        Line::from(format!("Update count: {}", state.update_count)),
        Line::from(format!("Podcasts: {}", state.library.len())),
        Line::from(format!("Episodes loaded: {}", state.episodes.len())),
        Line::from(format!("Queue length: {}", state.queue.len())),
        Line::from(""),
        Line::from("Settings panel is a work in progress."),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

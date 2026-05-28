use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, _state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");

    let lines = vec![
        Line::from(Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Tab / Shift+Tab    — switch tab"),
        Line::from("h / l  or  ← / →   — switch pane focus"),
        Line::from("j / k  or  ↓ / ↑   — navigate list"),
        Line::from("g / G              — jump to top / bottom"),
        Line::from(""),
        Line::from(Span::styled("Playback", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Space              — pause / resume"),
        Line::from("p                  — play from start"),
        Line::from(""),
        Line::from(Span::styled("Episode detail", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Enter              — open episode detail"),
        Line::from("j / k              — scroll description"),
        Line::from("p / d / s / a      — play / download / star / queue"),
        Line::from("Esc / q / h        — close detail"),
        Line::from(""),
        Line::from(Span::styled("Actions", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("d                  — download episode"),
        Line::from("s                  — star episode"),
        Line::from("S                  — unstar episode"),
        Line::from("a                  — add to queue"),
        Line::from("n                  — subscribe to feed"),
        Line::from("/                  — search iTunes"),
        Line::from(""),
        Line::from(Span::styled("Search tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Enter / s          — subscribe to selected result"),
        Line::from(""),
        Line::from(Span::styled("General", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("?                  — toggle help"),
        Line::from("q                  — quit"),
        Line::from("Ctrl+C             — quit"),
    ];

    let help = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    let popup_area = centered_rect(60, 80, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(help, popup_area);
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

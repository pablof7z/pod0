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
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Tab / Shift+Tab    — switch tab"),
        Line::from("h / l  or  ← / →   — switch pane focus"),
        Line::from("j / k  or  ↓ / ↑   — navigate list"),
        Line::from("g / G              — jump to top / bottom"),
        Line::from(""),
        Line::from(Span::styled(
            "Playback",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Space              — pause / resume"),
        Line::from("p                  — play from start"),
        Line::from(""),
        Line::from(Span::styled(
            "Episode detail",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Enter              — open episode detail"),
        Line::from("j / k              — scroll description"),
        Line::from("p / d / s / a / c  — play / download / star / queue / clip"),
        Line::from("t/H/u/m/f/C        — transcript / chapters / compile / summarize / comments"),
        Line::from("R / z / Z / x      — reset progress / 15m sleep / 30m sleep / cancel timer"),
        Line::from("Esc / q / h        — close detail"),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("d                  — download episode"),
        Line::from("D                  — delete completed episode download"),
        Line::from("s                  — star episode"),
        Line::from("S                  — unstar episode"),
        Line::from("a                  — add to queue"),
        Line::from("A                  — play next"),
        Line::from("c                  — AutoSnip current episode"),
        Line::from("n                  — subscribe to feed"),
        Line::from("/                  — search iTunes"),
        Line::from(""),
        Line::from(Span::styled(
            "Queue / clips / agent",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Queue: p / d / x   — play / remove / clear"),
        Line::from("Downloads: p/r/d/x — pause / resume / cancel / cancel all"),
        Line::from("Clips: p / d / c   — play / delete / clip now playing"),
        Line::from("Agent: h/l          — switch agent section"),
        Line::from("Agent: Enter / n   — compose or create row"),
        Line::from("Agent: r/e/d/x     — run-refresh / toggle / delete / clear"),
        Line::from("Settings: h/l      — switch general / providers / relays"),
        Line::from("Settings: Enter/e  — toggle or edit selected setting"),
        Line::from("Relays: n/d/r      — add / remove / cycle role"),
        Line::from(""),
        Line::from(Span::styled(
            "Search tab",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Enter / s          — subscribe to selected result"),
        Line::from(""),
        Line::from(Span::styled(
            "General",
            Style::default().add_modifier(Modifier::BOLD),
        )),
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

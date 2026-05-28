use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Player ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(ref np) = state.now_playing else {
        let empty = Paragraph::new("Nothing playing").alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // progress
    ])
    .split(inner);

    // Title line
    let status_indicator = if np.is_playing {
        Span::styled("▶ ", Style::default().fg(Color::Green))
    } else {
        Span::styled("⏸ ", Style::default().fg(Color::Yellow))
    };
    let title_line = Line::from(vec![
        status_indicator,
        Span::styled(
            format!("{} — {}", np.podcast_title, np.episode_title),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(title_line), rows[0]);

    // Progress
    let (pos_label, dur_label) = (format_time(np.position_secs), format_time(np.duration_secs));
    let ratio = if np.duration_secs > 0.0 {
        np.position_secs / np.duration_secs
    } else {
        0.0
    };

    let label = format!("{} / {}", pos_label, dur_label);
    let gauge = Gauge::default()
        .ratio(ratio.clamp(0.0, 1.0))
        .label(label)
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black));
    frame.render_widget(gauge, rows[1]);
}

fn format_time(secs: f64) -> String {
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

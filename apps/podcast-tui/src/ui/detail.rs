use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AppState, EpisodeRow, Mode};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Episode Detail ");

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let ep = match state.episodes.get(state.selected_episode) {
        Some(e) => e,
        None => {
            let empty = Paragraph::new("No episode selected").alignment(Alignment::Center);
            frame.render_widget(empty, inner);
            return;
        }
    };

    let scroll = match state.mode {
        Mode::EpisodeDetail { scroll } => scroll,
        _ => 0,
    };

    let lines = build_detail_lines(ep, state);
    let visible_lines = if lines.len() > scroll { &lines[scroll..] } else { &[] };

    let paragraph = Paragraph::new(visible_lines.to_vec())
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn build_detail_lines<'a>(ep: &'a EpisodeRow, state: &'a AppState) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::styled(
            &ep.title,
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));

    // Podcast + meta
    let mut meta_parts = Vec::new();
    if let Some(ref pt) = ep.podcast_title {
        meta_parts.push(pt.clone());
    }
    if let Some(dur) = ep.duration_secs {
        meta_parts.push(format_duration(dur));
    }
    if ep.played {
        meta_parts.push("played".to_string());
    }
    if ep.starred {
        meta_parts.push("starred".to_string());
    }
    if ep.download_path.is_some() {
        meta_parts.push("downloaded".to_string());
    }
    if ep.chapters_count > 0 {
        meta_parts.push(format!("{} chapters", ep.chapters_count));
    }
    if ep.has_transcript {
        meta_parts.push("transcript".to_string());
    }
    if !meta_parts.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            meta_parts.join(" | "),
            Style::default().fg(Color::DarkGray),
        )]));
    }

    lines.push(Line::from(""));

    // Now playing indicator
    if let Some(ref np) = state.now_playing {
        if np.episode_id == ep.id {
            let status = if np.is_playing { "▶ playing" } else { "⏸ paused" };
            lines.push(Line::from(vec![Span::styled(
                status,
                Style::default().fg(Color::Green),
            )]));
            if np.duration_secs > 0.0 {
                lines.push(Line::from(vec![Span::styled(
                    format!("position: {} / {}", format_time(np.position_secs), format_time(np.duration_secs)),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            lines.push(Line::from(""));
        }
    }

    // Description
    if let Some(ref desc) = ep.description {
        for paragraph in desc.split("\n\n") {
            for line in paragraph.lines() {
                lines.push(Line::from(line.to_string()));
            }
            lines.push(Line::from(""));
        }
    } else {
        lines.push(Line::from(vec![Span::styled(
            "No description available.",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    // Actions footer
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "p: play  |  d: download  |  s: star  |  a: queue  |  Esc: close",
        Style::default().fg(Color::DarkGray),
    )]));

    lines
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

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AppState, Tab};
use crate::ui::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    render_brand_bar(frame, rows[0], state);
    render_tab_bar(frame, rows[1], state);
}

fn render_brand_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let active = if state.agent_is_busy {
        Some(("agent", theme::ACCENT_ALT))
    } else if state.download_status_line().is_some() {
        Some(("downloads", theme::GOOD))
    } else if state.now_playing.as_ref().is_some_and(|np| np.is_playing) {
        Some(("playing", theme::ACCENT))
    } else {
        None
    };

    let mut spans = vec![
        theme::badge("Pod0", theme::ACCENT),
        Span::styled(" ", Style::default().bg(theme::BG)),
        Span::styled(
            format!(
                "{} {}",
                state.tab.label(),
                theme::wave(state.motion_tick, 8)
            ),
            Style::default()
                .fg(theme::pulse_color(state.motion_tick))
                .bg(theme::BG)
                .add_modifier(Modifier::BOLD),
        ),
        theme::separator(),
        Span::styled(
            format!("{} podcasts", state.library.len()),
            Style::default().fg(theme::TEXT).bg(theme::BG),
        ),
        theme::separator(),
        Span::styled(
            format!("{} queued", state.queue.len()),
            Style::default().fg(theme::TEXT).bg(theme::BG),
        ),
    ];

    if let Some((label, color)) = active {
        spans.push(theme::separator());
        spans.push(Span::styled(
            theme::spinner(state.motion_tick),
            Style::default().fg(color).bg(theme::BG),
        ));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default()
                .fg(color)
                .bg(theme::BG)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG));
    frame.render_widget(paragraph, area);
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let compact = area.width < 100;
    let mut tabs = vec![Span::styled(" ", Style::default().bg(theme::SURFACE))];

    tabs.extend(
        Tab::all()
            .iter()
            .map(|tab| tab_chip(tab, state, compact))
            .collect::<Vec<_>>(),
    );

    if let Some(dl_status) = state.download_status_line() {
        tabs.push(Span::styled("  ", Style::default().bg(theme::SURFACE)));
        tabs.push(Span::styled(
            theme::spinner(state.motion_tick),
            Style::default()
                .fg(theme::pulse_color(state.motion_tick))
                .bg(theme::SURFACE),
        ));
        tabs.push(Span::styled(
            format!(" {dl_status}"),
            Style::default()
                .fg(theme::GOOD)
                .bg(theme::SURFACE)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let paragraph = Paragraph::new(vec![Line::from(tabs)])
        .alignment(Alignment::Left)
        .style(Style::default().bg(theme::SURFACE));
    frame.render_widget(paragraph, area);
}

fn tab_chip(tab: &Tab, state: &AppState, compact: bool) -> Span<'static> {
    let mut label = if compact && *tab != state.tab {
        compact_tab_label(tab).to_owned()
    } else {
        tab.label().to_owned()
    };
    if *tab == Tab::Agent && state.agent_is_busy {
        label.push(' ');
        label.push_str(theme::spinner(state.motion_tick));
    }
    let label = format!(" {label} ");
    if *tab == state.tab {
        Span::styled(label, theme::selected())
    } else {
        Span::styled(label, Style::default().fg(theme::MUTED).bg(theme::SURFACE))
    }
}

fn compact_tab_label(tab: &Tab) -> &'static str {
    match tab {
        Tab::Library => "lib",
        Tab::Queue => "q",
        Tab::Inbox => "in",
        Tab::Search => "find",
        Tab::Downloads => "dl",
        Tab::Bookmarks => "stars",
        Tab::Clips => "clip",
        Tab::Agent => "ai",
        Tab::Social => "soc",
        Tab::Settings => "cfg",
    }
}

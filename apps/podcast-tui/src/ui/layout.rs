use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, Mode, Tab};
use crate::ui::{self, theme};

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(theme::BG)),
        area,
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // title bar
            Constraint::Min(6),    // body
            Constraint::Length(6), // player
            Constraint::Length(1), // status
        ])
        .split(area);

    ui::topbar::render(frame, rows[0], state);
    render_body(frame, rows[1], state);
    ui::player::render(frame, rows[2], state);
    render_status(frame, rows[3], state);

    if state.show_help {
        ui::help::render(frame, area, state);
    }

    if matches!(
        state.mode,
        Mode::SearchInput
            | Mode::SubscribeInput
            | Mode::RelayInput
            | Mode::SettingsInput
            | Mode::AgentInput
            | Mode::AgentMemoryInput
            | Mode::AgentTaskInput
            | Mode::AgentNoteInput
            | Mode::EpisodeCommentInput
    ) {
        render_input_bar(frame, area, state);
    }

    if matches!(state.mode, Mode::ProviderCatalog) {
        ui::provider_catalog::render(frame, area, state);
    }

    if matches!(state.mode, Mode::EpisodeDetail { .. }) {
        let popup = Layout::vertical([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(area);
        let detail_area = Layout::horizontal([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(popup[1])[1];
        ui::detail::render(frame, detail_area, state);
    }
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.tab {
        Tab::Library => render_library_body(frame, area, state),
        Tab::Queue => ui::queue::render(frame, area, state),
        Tab::Inbox => ui::inbox::render(frame, area, state),
        Tab::Search => ui::search::render(frame, area, state),
        Tab::Downloads => ui::downloads::render(frame, area, state),
        Tab::Bookmarks => ui::bookmarks::render(frame, area, state),
        Tab::Clips => ui::clips::render(frame, area, state),
        Tab::Agent => ui::agent::render(frame, area, state),
        Tab::Social => ui::social::render(frame, area, state),
        Tab::Settings => ui::settings::render(frame, area, state),
    }
}

fn render_library_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let cols =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(area);

    ui::library::render(frame, cols[0], state);
    ui::episodes::render(frame, cols[1], state);
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let mut spans = vec![];

    if let Some(dl_status) = state.download_status_line() {
        spans.push(Span::styled(
            theme::spinner(state.motion_tick),
            Style::default().fg(theme::pulse_color(state.motion_tick)),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            dl_status,
            Style::default()
                .fg(theme::GOOD)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(theme::separator());
    }

    if state.agent_is_busy {
        spans.push(Span::styled(
            theme::spinner(state.motion_tick),
            Style::default().fg(theme::ACCENT_ALT),
        ));
        spans.push(Span::styled(" agent busy", theme::accent()));
        spans.push(theme::separator());
    }

    spans.push(Span::styled(&state.status, theme::muted()));

    if let Some(ref toast) = state.toasts.last() {
        spans.push(theme::separator());
        spans.push(Span::styled("notice ", theme::warn()));
        spans.push(Span::styled(
            &toast.message,
            Style::default().fg(theme::TEXT),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(vec![line]).style(Style::default().bg(theme::BG));
    frame.render_widget(paragraph, area);
}

fn render_input_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let label = match state.mode {
        Mode::SearchInput => "Search: ",
        Mode::SubscribeInput => "Subscribe: ",
        Mode::RelayInput => "Relay: ",
        Mode::SettingsInput => "Setting: ",
        Mode::AgentInput => "Agent: ",
        Mode::AgentMemoryInput => "Memory key=value: ",
        Mode::AgentTaskInput => "Task: ",
        Mode::AgentNoteInput => "Note: ",
        Mode::EpisodeCommentInput => "Comment: ",
        _ => return,
    };
    let value = match state.mode {
        Mode::SearchInput => &state.search_input,
        Mode::SubscribeInput => &state.subscribe_input,
        Mode::RelayInput => &state.relay_input,
        Mode::SettingsInput => &state.settings_input,
        Mode::AgentInput => &state.agent_input,
        Mode::AgentMemoryInput => &state.agent_memory_input,
        Mode::AgentTaskInput => &state.agent_task_input,
        Mode::AgentNoteInput => &state.agent_note_input,
        Mode::EpisodeCommentInput => &state.episode_comment_input,
        _ => return,
    };

    let popup = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Length(3),
        Constraint::Percentage(50),
    ])
    .split(area);

    let input_area = Layout::horizontal([
        Constraint::Percentage(10),
        Constraint::Percentage(80),
        Constraint::Percentage(10),
    ])
    .split(popup[1])[1];

    let text = format!("{}{}", label, value);
    let block = theme::panel_with_footer("Input", "Enter submit  Esc cancel", true);
    let paragraph = Paragraph::new(text).style(theme::text()).block(block);
    frame.render_widget(Clear, input_area);
    frame.render_widget(paragraph, input_area);
}

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, Mode, Tab};
use crate::ui;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(6),    // body
            Constraint::Length(3), // player
            Constraint::Length(1), // status
        ])
        .split(area);

    render_title(frame, rows[0], state);
    render_body(frame, rows[1], state);
    ui::player::render(frame, rows[2], state);
    render_status(frame, rows[3], state);

    if state.show_help {
        ui::help::render(frame, area, state);
    }

    if matches!(state.mode, Mode::SearchInput | Mode::SubscribeInput) {
        render_input_bar(frame, area, state);
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

fn render_title(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let tabs = [Tab::Library, Tab::Queue, Tab::Inbox, Tab::Search, Tab::Settings]
        .iter()
        .map(|tab| {
            let label = format!(" {} ", tab.label());
            if *tab == state.tab {
                Span::styled(
                    label,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(label, Style::default().fg(Color::DarkGray))
            }
        })
        .collect::<Vec<_>>();

    let line = Line::from(tabs);
    let paragraph = Paragraph::new(vec![line]).alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.tab {
        Tab::Library => render_library_body(frame, area, state),
        Tab::Queue => ui::queue::render(frame, area, state),
        Tab::Inbox => ui::inbox::render(frame, area, state),
        Tab::Search => ui::search::render(frame, area, state),
        Tab::Settings => ui::settings::render(frame, area, state),
    }
}

fn render_library_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let cols = Layout::horizontal([
        Constraint::Percentage(35),
        Constraint::Percentage(65),
    ])
    .split(area);

    ui::library::render(frame, cols[0], state);
    ui::episodes::render(frame, cols[1], state);
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let mut spans = vec![];

    // Download progress indicator (tasteful, only when active)
    let dl_status = state.download_status_line();
    if let Some(ref dl_status) = dl_status {
        spans.push(Span::styled(
            dl_status,
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
    }

    spans.push(Span::styled(&state.status, Style::default().fg(Color::Gray)));

    if let Some(ref toast) = state.toasts.last() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            &toast.message,
            Style::default().fg(Color::Yellow),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(vec![line]);
    frame.render_widget(paragraph, area);
}

fn render_input_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let label = match state.mode {
        Mode::SearchInput => "Search: ",
        Mode::SubscribeInput => "Subscribe: ",
        _ => return,
    };
    let value = match state.mode {
        Mode::SearchInput => &state.search_input,
        Mode::SubscribeInput => &state.subscribe_input,
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title("Input");
    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, input_area);
}

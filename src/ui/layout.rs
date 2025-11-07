use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    // Main layout: top bar, content, status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Top bar
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Status bar
        ])
        .split(size);

    // Render top bar
    render_top_bar(frame, chunks[0], app);

    // Content layout: sidebar and main area
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar
            Constraint::Percentage(75), // Main area
        ])
        .split(chunks[1]);

    // Render database browser in sidebar
    app.database_browser.render(frame, content_chunks[0]);

    // Main area layout: query editor and results
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // Query editor
            Constraint::Percentage(60), // Results
        ])
        .split(content_chunks[1]);

    // Render query editor
    app.query_editor.render(frame, main_chunks[0]);

    // Render results viewer
    app.results_viewer.render(frame, main_chunks[1]);

    // Render status bar
    render_status_bar(frame, chunks[2], app);

    // Render connection manager popup (if visible)
    let connections: Vec<(String, String, String)> = app.config.get_connections()
        .iter()
        .map(|c| (c.name.clone(), c.db_type.clone(), c.connection_string.clone()))
        .collect();
    app.connection_manager.render(frame, size, &connections);
}

fn render_top_bar(frame: &mut Frame, area: Rect, _app: &App) {
    let title = vec![
        Span::styled("TUI-DB", Style::default().fg(Color::Cyan)),
        Span::raw(" - Terminal Database Manager"),
    ];

    let paragraph = Paragraph::new(Line::from(title))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default());

    frame.render_widget(paragraph, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mode_color = match app.vim_state.mode {
        crate::vim::VimMode::Normal => Color::Blue,
        crate::vim::VimMode::Insert => Color::Green,
        crate::vim::VimMode::Visual => Color::Magenta,
        crate::vim::VimMode::Command => Color::Yellow,
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.vim_state.mode.as_str()),
            Style::default().fg(Color::Black).bg(mode_color),
        ),
        Span::raw("  "),
    ];

    // Show active pane
    let active_pane = match app.active_pane {
        crate::app::Pane::DatabaseBrowser => "Database Browser",
        crate::app::Pane::QueryEditor => "Query Editor",
        crate::app::Pane::Results => "Results",
    };
    spans.push(Span::styled(
        format!("[{}]", active_pane),
        Style::default().fg(Color::Cyan),
    ));

    // Show command buffer if in command mode
    if app.vim_state.mode == crate::vim::VimMode::Command {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(":{}", app.vim_state.get_command()),
            Style::default().fg(Color::Yellow),
        ));
    }

    // Show help hint
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "Press ':' for commands, 'q' to quit",
        Style::default().fg(Color::DarkGray),
    ));

    let paragraph = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default());

    frame.render_widget(paragraph, area);
}

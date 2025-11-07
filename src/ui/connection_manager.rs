use crate::db::DatabaseType;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionManagerMode {
    List,
    Add,
    Edit(usize), // editing connection at index
    Test,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormField {
    Name,
    Type,
    ConnectionString,
}

#[derive(Debug, Clone)]
pub struct ConnectionForm {
    pub name: String,
    pub db_type: DatabaseType,
    pub connection_string: String,
    pub active_field: FormField,
}

impl Default for ConnectionForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            db_type: DatabaseType::SQLite,
            connection_string: String::new(),
            active_field: FormField::Name,
        }
    }
}

#[derive(Debug)]
pub struct ConnectionManager {
    pub visible: bool,
    pub mode: ConnectionManagerMode,
    pub form: ConnectionForm,
    pub list_state: ListState,
    pub test_result: Option<String>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            visible: false,
            mode: ConnectionManagerMode::List,
            form: ConnectionForm::default(),
            list_state,
            test_result: None,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.mode = ConnectionManagerMode::List;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.test_result = None;
    }

    pub fn show_add_form(&mut self) {
        self.mode = ConnectionManagerMode::Add;
        self.form = ConnectionForm::default();
        self.test_result = None;
    }

    pub fn show_edit_form(&mut self, index: usize, name: String, db_type: DatabaseType, connection_string: String) {
        self.mode = ConnectionManagerMode::Edit(index);
        self.form = ConnectionForm {
            name,
            db_type,
            connection_string,
            active_field: FormField::Name,
        };
        self.test_result = None;
    }

    pub fn next_field(&mut self) {
        self.form.active_field = match self.form.active_field {
            FormField::Name => FormField::Type,
            FormField::Type => FormField::ConnectionString,
            FormField::ConnectionString => FormField::Name,
        };
    }

    pub fn prev_field(&mut self) {
        self.form.active_field = match self.form.active_field {
            FormField::Name => FormField::ConnectionString,
            FormField::Type => FormField::Name,
            FormField::ConnectionString => FormField::Type,
        };
    }

    pub fn cycle_db_type(&mut self) {
        self.form.db_type = match self.form.db_type {
            DatabaseType::SQLite => DatabaseType::MySQL,
            DatabaseType::MySQL => DatabaseType::MariaDB,
            DatabaseType::MariaDB => DatabaseType::SQLite,
        };
    }

    pub fn insert_char(&mut self, c: char) {
        match self.form.active_field {
            FormField::Name => self.form.name.push(c),
            FormField::ConnectionString => self.form.connection_string.push(c),
            FormField::Type => {}, // Type is cycled, not typed
        }
    }

    pub fn delete_char(&mut self) {
        match self.form.active_field {
            FormField::Name => { self.form.name.pop(); },
            FormField::ConnectionString => { self.form.connection_string.pop(); },
            FormField::Type => {},
        }
    }

    pub fn move_list_up(&mut self) {
        let selected = self.list_state.selected().unwrap_or(0);
        if selected > 0 {
            self.list_state.select(Some(selected - 1));
        }
    }

    pub fn move_list_down(&mut self, max: usize) {
        let selected = self.list_state.selected().unwrap_or(0);
        if selected < max.saturating_sub(1) {
            self.list_state.select(Some(selected + 1));
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, connections: &[(String, String, String)]) {
        if !self.visible {
            return;
        }

        // Create centered popup area
        let popup_area = centered_rect(80, 70, area);

        // Clear the area
        frame.render_widget(Clear, popup_area);

        match &self.mode {
            ConnectionManagerMode::List => self.render_list(frame, popup_area, connections),
            ConnectionManagerMode::Add => self.render_form(frame, popup_area, "Add Connection"),
            ConnectionManagerMode::Edit(_) => self.render_form(frame, popup_area, "Edit Connection"),
            ConnectionManagerMode::Test => self.render_test_result(frame, popup_area),
        }
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, connections: &[(String, String, String)]) {
        let block = Block::default()
            .title(" Connection Manager ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(inner);

        // Connection list
        let items: Vec<ListItem> = connections
            .iter()
            .map(|(name, db_type, _)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}] ", db_type), Style::default().fg(Color::Yellow)),
                    Span::raw(name),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");

        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        // Help text
        let help = Paragraph::new(vec![
            Line::from("n: New  e: Edit  d: Delete  t: Test  Enter: Connect  Esc: Close"),
        ])
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

        frame.render_widget(help, chunks[1]);
    }

    fn render_form(&mut self, frame: &mut Frame, area: Rect, title: &str) {
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .margin(1)
            .split(inner);

        // Name field
        let name_style = if self.form.active_field == FormField::Name {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let name_block = Block::default()
            .title("Name")
            .borders(Borders::ALL)
            .border_style(name_style);
        let name_text = Paragraph::new(self.form.name.as_str()).block(name_block);
        frame.render_widget(name_text, chunks[0]);

        // Type field
        let type_style = if self.form.active_field == FormField::Type {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let type_text = match self.form.db_type {
            DatabaseType::SQLite => "SQLite",
            DatabaseType::MySQL => "MySQL",
            DatabaseType::MariaDB => "MariaDB",
        };
        let type_block = Block::default()
            .title("Database Type (Space to cycle)")
            .borders(Borders::ALL)
            .border_style(type_style);
        let type_para = Paragraph::new(type_text).block(type_block);
        frame.render_widget(type_para, chunks[1]);

        // Connection String field
        let conn_style = if self.form.active_field == FormField::ConnectionString {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let conn_label = match self.form.db_type {
            DatabaseType::SQLite => "File Path",
            DatabaseType::MySQL | DatabaseType::MariaDB => "Connection String (mysql://user:pass@host:port/db)",
        };
        let conn_block = Block::default()
            .title(conn_label)
            .borders(Borders::ALL)
            .border_style(conn_style);
        let conn_text = Paragraph::new(self.form.connection_string.as_str())
            .block(conn_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(conn_text, chunks[2]);

        // Test result
        if let Some(ref result) = self.test_result {
            let result_color = if result.contains("Success") {
                Color::Green
            } else {
                Color::Red
            };
            let result_para = Paragraph::new(result.as_str())
                .style(Style::default().fg(result_color))
                .wrap(Wrap { trim: false });
            frame.render_widget(result_para, chunks[3]);
        }

        // Help text
        let help = Paragraph::new(vec![
            Line::from("Tab: Next field  Shift+Tab: Prev field  Space: Cycle type (on Type field)"),
            Line::from("Ctrl+T: Test  Ctrl+S: Save  Esc: Cancel"),
        ])
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

        frame.render_widget(help, chunks[4]);
    }

    fn render_test_result(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Testing Connection... ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let text = Paragraph::new("Testing connection, please wait...")
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(text, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

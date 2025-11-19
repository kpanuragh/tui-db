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
    #[allow(dead_code)]
    Test,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormField {
    Name,
    Type,
    ConnectionString, // For SQLite file path
    Username,        // For MySQL/MariaDB
    Password,        // For MySQL/MariaDB
    Host,           // For MySQL/MariaDB
    Port,           // For MySQL/MariaDB
    Database,       // For MySQL/MariaDB (optional)
}

#[derive(Debug, Clone)]
pub struct ConnectionForm {
    pub name: String,
    pub db_type: DatabaseType,
    pub connection_string: String, // Used for SQLite file path
    pub username: String,          // For MySQL/MariaDB
    pub password: String,          // For MySQL/MariaDB
    pub host: String,             // For MySQL/MariaDB
    pub port: String,             // For MySQL/MariaDB
    pub database: String,         // For MySQL/MariaDB (optional)
    pub active_field: FormField,
}

impl Default for ConnectionForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            db_type: DatabaseType::SQLite,
            connection_string: String::new(),
            username: String::new(),
            password: String::new(),
            host: "localhost".to_string(),
            port: "3306".to_string(),
            database: String::new(),
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
            db_type: db_type.clone(),
            connection_string: connection_string.clone(),
            username: String::new(),
            password: String::new(),
            host: "localhost".to_string(),
            port: "3306".to_string(),
            database: String::new(),
            active_field: FormField::Name,
        };
        
        // Parse connection string for MySQL/MariaDB to populate individual fields
        if matches!(db_type, DatabaseType::MySQL | DatabaseType::MariaDB) {
            self.parse_connection_string(&connection_string);
        }
        
        self.test_result = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show_edit_form_detailed(&mut self, index: usize, name: String, db_type: DatabaseType, connection_string: String, username: Option<String>, password: Option<String>, host: Option<String>, port: Option<String>, database: Option<String>) {
        self.mode = ConnectionManagerMode::Edit(index);
        self.form = ConnectionForm {
            name,
            db_type,
            connection_string,
            username: username.unwrap_or_default(),
            password: password.unwrap_or_default(),
            host: host.unwrap_or_else(|| "localhost".to_string()),
            port: port.unwrap_or_else(|| "3306".to_string()),
            database: database.unwrap_or_default(),
            active_field: FormField::Name,
        };
        
        self.test_result = None;
    }

    fn parse_connection_string(&mut self, connection_string: &str) {
        // Parse MySQL connection string: mysql://user:pass@host:port/database
        if let Some(stripped) = connection_string.strip_prefix("mysql://") {
            let mut parts = stripped.splitn(2, '@');
            
            if let Some(credentials) = parts.next() {
                let mut cred_parts = credentials.splitn(2, ':');
                if let Some(username) = cred_parts.next() {
                    self.form.username = username.to_string();
                    if let Some(password) = cred_parts.next() {
                        self.form.password = password.to_string();
                    }
                }
            }
            
            if let Some(host_db) = parts.next() {
                let mut host_db_parts = host_db.splitn(2, '/');
                if let Some(host_port) = host_db_parts.next() {
                    let mut hp_parts = host_port.splitn(2, ':');
                    if let Some(host) = hp_parts.next() {
                        self.form.host = host.to_string();
                        if let Some(port) = hp_parts.next() {
                            self.form.port = port.to_string();
                        }
                    }
                }
                
                if let Some(database) = host_db_parts.next() {
                    self.form.database = database.to_string();
                }
            }
        }
    }

    pub fn next_field(&mut self) {
        self.form.active_field = match self.form.db_type {
            DatabaseType::SQLite => match self.form.active_field {
                FormField::Name => FormField::Type,
                FormField::Type => FormField::ConnectionString,
                FormField::ConnectionString => FormField::Name,
                _ => FormField::Name, // Fallback for invalid states
            },
            DatabaseType::MySQL | DatabaseType::MariaDB => match self.form.active_field {
                FormField::Name => FormField::Type,
                FormField::Type => FormField::Host,
                FormField::Host => FormField::Port,
                FormField::Port => FormField::Username,
                FormField::Username => FormField::Password,
                FormField::Password => FormField::Database,
                FormField::Database => FormField::Name,
                _ => FormField::Name, // Fallback for invalid states
            },
        };
    }

    pub fn prev_field(&mut self) {
        self.form.active_field = match self.form.db_type {
            DatabaseType::SQLite => match self.form.active_field {
                FormField::Name => FormField::ConnectionString,
                FormField::Type => FormField::Name,
                FormField::ConnectionString => FormField::Type,
                _ => FormField::Name, // Fallback for invalid states
            },
            DatabaseType::MySQL | DatabaseType::MariaDB => match self.form.active_field {
                FormField::Name => FormField::Database,
                FormField::Type => FormField::Name,
                FormField::Host => FormField::Type,
                FormField::Port => FormField::Host,
                FormField::Username => FormField::Port,
                FormField::Password => FormField::Username,
                FormField::Database => FormField::Password,
                _ => FormField::Name, // Fallback for invalid states
            },
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
            FormField::Username => self.form.username.push(c),
            FormField::Password => self.form.password.push(c),
            FormField::Host => self.form.host.push(c),
            FormField::Port => {
                // Only allow digits for port
                if c.is_ascii_digit() {
                    self.form.port.push(c);
                }
            },
            FormField::Database => self.form.database.push(c),
            FormField::Type => {}, // Type is cycled, not typed
        }
    }

    pub fn delete_char(&mut self) {
        match self.form.active_field {
            FormField::Name => { self.form.name.pop(); },
            FormField::ConnectionString => { self.form.connection_string.pop(); },
            FormField::Username => { self.form.username.pop(); },
            FormField::Password => { self.form.password.pop(); },
            FormField::Host => { self.form.host.pop(); },
            FormField::Port => { self.form.port.pop(); },
            FormField::Database => { self.form.database.pop(); },
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

    pub fn get_connection_string(&self) -> String {
        match self.form.db_type {
            DatabaseType::SQLite => self.form.connection_string.clone(),
            DatabaseType::MySQL | DatabaseType::MariaDB => {
                let protocol = match self.form.db_type {
                    DatabaseType::MySQL => "mysql",
                    DatabaseType::MariaDB => "mysql", // MariaDB uses mysql protocol
                    _ => unreachable!(),
                };
                
                let mut url = format!("{}://", protocol);
                
                // Add username and password if provided
                if !self.form.username.is_empty() {
                    url.push_str(&self.form.username);
                    if !self.form.password.is_empty() {
                        url.push(':');
                        url.push_str(&self.form.password);
                    }
                    url.push('@');
                }
                
                // Add host
                let host = if self.form.host.is_empty() { "localhost" } else { &self.form.host };
                url.push_str(host);
                
                // Add port if not default
                let port = if self.form.port.is_empty() { "3306" } else { &self.form.port };
                if port != "3306" {
                    url.push(':');
                    url.push_str(port);
                }
                
                // Add database if provided
                if !self.form.database.is_empty() {
                    url.push('/');
                    url.push_str(&self.form.database);
                }
                
                url
            }
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

        match self.form.db_type {
            DatabaseType::SQLite => self.render_sqlite_form(frame, inner),
            DatabaseType::MySQL | DatabaseType::MariaDB => self.render_mysql_form(frame, inner),
        }
    }

    fn render_sqlite_form(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Type
                Constraint::Length(3), // File Path
                Constraint::Min(1),    // Test result
                Constraint::Length(4), // Help
            ])
            .margin(1)
            .split(area);

        // Name field
        let name_style = if self.form.active_field == FormField::Name {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let name_block = Block::default()
            .title("Connection Name")
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
        let type_block = Block::default()
            .title("Database Type (Space to cycle)")
            .borders(Borders::ALL)
            .border_style(type_style);
        let type_para = Paragraph::new("SQLite").block(type_block);
        frame.render_widget(type_para, chunks[1]);

        // File Path field
        let path_style = if self.form.active_field == FormField::ConnectionString {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let path_block = Block::default()
            .title("Database File Path")
            .borders(Borders::ALL)
            .border_style(path_style);
        let path_text = Paragraph::new(self.form.connection_string.as_str())
            .block(path_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(path_text, chunks[2]);

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

    fn render_mysql_form(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Type
                Constraint::Length(6), // Host and Port row
                Constraint::Length(6), // Username and Password row
                Constraint::Length(3), // Database
                Constraint::Min(1),    // Test result
                Constraint::Length(4), // Help
            ])
            .margin(1)
            .split(area);

        // Name field
        let name_style = if self.form.active_field == FormField::Name {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let name_block = Block::default()
            .title("Connection Name")
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
            DatabaseType::MySQL => "MySQL",
            DatabaseType::MariaDB => "MariaDB",
            _ => "MySQL",
        };
        let type_block = Block::default()
            .title("Database Type (Space to cycle)")
            .borders(Borders::ALL)
            .border_style(type_style);
        let type_para = Paragraph::new(type_text).block(type_block);
        frame.render_widget(type_para, chunks[1]);

        // Host and Port row
        let host_port_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[2]);

        let host_style = if self.form.active_field == FormField::Host {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let host_block = Block::default()
            .title("Host")
            .borders(Borders::ALL)
            .border_style(host_style);
        let host_text = Paragraph::new(self.form.host.as_str()).block(host_block);
        frame.render_widget(host_text, host_port_chunks[0]);

        let port_style = if self.form.active_field == FormField::Port {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let port_block = Block::default()
            .title("Port")
            .borders(Borders::ALL)
            .border_style(port_style);
        let port_text = Paragraph::new(self.form.port.as_str()).block(port_block);
        frame.render_widget(port_text, host_port_chunks[1]);

        // Username and Password row
        let user_pass_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[3]);

        let username_style = if self.form.active_field == FormField::Username {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let username_block = Block::default()
            .title("Username")
            .borders(Borders::ALL)
            .border_style(username_style);
        let username_text = Paragraph::new(self.form.username.as_str()).block(username_block);
        frame.render_widget(username_text, user_pass_chunks[0]);

        let password_style = if self.form.active_field == FormField::Password {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let password_block = Block::default()
            .title("Password")
            .borders(Borders::ALL)
            .border_style(password_style);
        // Mask password with asterisks
        let password_display = "*".repeat(self.form.password.len());
        let password_text = Paragraph::new(password_display.as_str()).block(password_block);
        frame.render_widget(password_text, user_pass_chunks[1]);

        // Database field
        let database_style = if self.form.active_field == FormField::Database {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let database_block = Block::default()
            .title("Database (Optional)")
            .borders(Borders::ALL)
            .border_style(database_style);
        let database_text = Paragraph::new(self.form.database.as_str()).block(database_block);
        frame.render_widget(database_text, chunks[4]);

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
            frame.render_widget(result_para, chunks[5]);
        }

        // Help text
        let help = Paragraph::new(vec![
            Line::from("Tab: Next field  Shift+Tab: Prev field  Space: Cycle type (on Type field)"),
            Line::from("Ctrl+T: Test  Ctrl+S: Save  Esc: Cancel"),
        ])
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

        frame.render_widget(help, chunks[6]);
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

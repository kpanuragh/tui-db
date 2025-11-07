use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::collections::HashMap;
use std::time::Duration;

use crate::config::Config;
use crate::db::{sqlite::SQLiteConnection, mysql::MySQLConnection, ConnectionInfo, DatabaseConnection, DatabaseType};
use crate::ui::{DatabaseBrowser, QueryEditor, ResultsViewer, ConnectionManager};
use crate::vim::{VimCommand, VimMode, VimState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    DatabaseBrowser,
    QueryEditor,
    Results,
}

pub struct App {
    pub should_quit: bool,
    pub vim_state: VimState,
    pub active_pane: Pane,
    pub database_browser: DatabaseBrowser,
    pub query_editor: QueryEditor,
    pub results_viewer: ResultsViewer,
    pub connection_manager: ConnectionManager,
    pub config: Config,
    pub connections: HashMap<usize, Box<dyn DatabaseConnection>>,
    pub next_connection_id: usize,
    pub clipboard: Option<String>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let mut app = Self {
            should_quit: false,
            vim_state: VimState::new(),
            active_pane: Pane::DatabaseBrowser,
            database_browser: DatabaseBrowser::new(),
            query_editor: QueryEditor::new(),
            results_viewer: ResultsViewer::new(),
            connection_manager: ConnectionManager::new(),
            config,
            connections: HashMap::new(),
            next_connection_id: 0,
            clipboard: None,
        };

        // Update focused states
        app.update_focus();

        // Load saved connections
        app.load_saved_connections()?;

        Ok(app)
    }

    pub fn handle_events(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                self.handle_key_event(key)?;
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};
        use crate::ui::connection_manager::ConnectionManagerMode;

        // If connection manager is visible, handle keys differently
        if self.connection_manager.visible {
            match key.code {
                KeyCode::Esc => {
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        self.connection_manager.mode = ConnectionManagerMode::List;
                        self.connection_manager.test_result = None;
                    } else {
                        self.connection_manager.hide();
                    }
                }
                KeyCode::Enter => {
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::List) {
                        self.handle_connection_manager_action('\n')?;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    match self.connection_manager.mode {
                        ConnectionManagerMode::List => {
                            self.handle_connection_manager_action(c)?;
                        }
                        ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_) => {
                            match c {
                                ' ' if self.connection_manager.form.active_field == crate::ui::connection_manager::FormField::Type => {
                                    self.connection_manager.cycle_db_type();
                                }
                                _ => {
                                    self.connection_manager.insert_char(c);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Backspace => {
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        self.connection_manager.delete_char();
                    }
                }
                KeyCode::Tab => {
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        self.connection_manager.next_field();
                    }
                }
                KeyCode::BackTab => {
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        self.connection_manager.prev_field();
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if matches!(self.connection_manager.mode, ConnectionManagerMode::List) => {
                    self.connection_manager.move_list_up();
                }
                KeyCode::Down | KeyCode::Char('j') if matches!(self.connection_manager.mode, ConnectionManagerMode::List) => {
                    let max = self.config.get_connections().len();
                    self.connection_manager.move_list_down(max);
                }
                KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Test connection from form
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        let conn_str = self.connection_manager.form.connection_string.clone();
                        let db_type = self.connection_manager.form.db_type.clone();
                        self.test_connection(&conn_str, db_type)?;
                    }
                }
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Save connection from form
                    if matches!(self.connection_manager.mode, ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_)) {
                        self.save_connection_from_form()?;
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        // Normal key handling when connection manager is not visible
        if let Some(vim_command) = self.vim_state.handle_key(key) {
            self.execute_vim_command(vim_command)?;
        }
        Ok(())
    }

    fn execute_vim_command(&mut self, command: VimCommand) -> Result<()> {
        match command {
            VimCommand::Quit => {
                self.should_quit = true;
            }
            VimCommand::ExecuteCommand(cmd) => {
                self.execute_command(&cmd)?;
            }
            VimCommand::NextPane => {
                self.next_pane();
            }
            VimCommand::PrevPane => {
                self.prev_pane();
            }
            VimCommand::Activate => {
                if self.active_pane == Pane::DatabaseBrowser {
                    self.load_selected_table_data()?;
                }
            }
            VimCommand::ExecuteQueryUnderCursor => {
                if self.active_pane == Pane::QueryEditor {
                    self.execute_query_at_cursor()?;
                }
            }
            VimCommand::ExecuteAllQueries => {
                if self.active_pane == Pane::QueryEditor {
                    self.execute_query()?;
                }
            }
            VimCommand::EnterInsertMode | VimCommand::EnterInsertModeAfter => {
                if self.active_pane == Pane::QueryEditor {
                    self.vim_state.enter_insert_mode();
                }
            }
            VimCommand::ExitInsertMode => {
                self.vim_state.enter_normal_mode();
                // If we're in results viewer edit mode, save the current cell
                if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    self.results_viewer.save_cell_edit();
                }
                // If we're in results viewer insert mode, save the current field
                if self.active_pane == Pane::Results && self.results_viewer.insert_mode {
                    self.results_viewer.save_insert_field();
                }
            }
            VimCommand::EnterInsertRowMode => {
                if self.active_pane == Pane::Results {
                    self.results_viewer.enter_insert_mode();
                    self.vim_state.enter_insert_mode();
                }
            }
            VimCommand::SaveInsertRow => {
                if self.active_pane == Pane::Results {
                    self.save_insert_row()?;
                }
            }
            VimCommand::EnterEditMode => {
                if self.active_pane == Pane::Results {
                    self.results_viewer.enter_edit_mode();
                    self.vim_state.enter_insert_mode();
                }
            }
            VimCommand::ExitEditMode => {
                if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    self.results_viewer.exit_edit_mode();
                    self.vim_state.enter_normal_mode();
                }
            }
            VimCommand::MoveColumnLeft => {
                if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    self.results_viewer.move_column_left();
                }
            }
            VimCommand::MoveColumnRight => {
                if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    self.results_viewer.move_column_right();
                }
            }
            VimCommand::SaveAllEdits => {
                if self.active_pane == Pane::Results {
                    if self.results_viewer.insert_mode {
                        self.save_insert_row()?;
                    } else {
                        self.save_table_edits()?;
                    }
                }
            }
            VimCommand::EnterVisualMode => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.start_visual_mode();
                }
            }
            VimCommand::ExitVisualMode => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.exit_visual_mode();
                }
            }
            VimCommand::MoveUp(count) => match self.active_pane {
                Pane::DatabaseBrowser => {
                    for _ in 0..count {
                        self.database_browser.move_up();
                    }
                }
                Pane::QueryEditor => {
                    self.query_editor.move_up(count);
                }
                Pane::Results => {
                    self.results_viewer.move_up(count);
                }
            },
            VimCommand::MoveDown(count) => match self.active_pane {
                Pane::DatabaseBrowser => {
                    for _ in 0..count {
                        self.database_browser.move_down();
                    }
                }
                Pane::QueryEditor => {
                    self.query_editor.move_down(count);
                }
                Pane::Results => {
                    self.results_viewer.move_down(count);
                }
            },
            VimCommand::MoveLeft(count) => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.move_left(count);
                } else if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    for _ in 0..count {
                        self.results_viewer.save_cell_edit();
                        self.results_viewer.move_column_left();
                    }
                } else if self.active_pane == Pane::Results && self.results_viewer.insert_mode {
                    for _ in 0..count {
                        self.results_viewer.save_insert_field();
                        self.results_viewer.move_column_left();
                    }
                }
            }
            VimCommand::MoveRight(count) => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.move_right(count);
                } else if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                    for _ in 0..count {
                        self.results_viewer.save_cell_edit();
                        self.results_viewer.move_column_right();
                    }
                } else if self.active_pane == Pane::Results && self.results_viewer.insert_mode {
                    for _ in 0..count {
                        self.results_viewer.save_insert_field();
                        self.results_viewer.move_column_right();
                    }
                }
            }
            VimCommand::GotoTop => match self.active_pane {
                Pane::QueryEditor => self.query_editor.goto_top(),
                Pane::Results => self.results_viewer.goto_top(),
                _ => {}
            },
            VimCommand::GotoBottom => match self.active_pane {
                Pane::QueryEditor => self.query_editor.goto_bottom(),
                Pane::Results => self.results_viewer.goto_bottom(),
                _ => {}
            },
            VimCommand::GotoLineStart => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.goto_line_start();
                }
            }
            VimCommand::GotoLineEnd => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.goto_line_end();
                }
            }
            VimCommand::InsertChar(c) => {
                if self.vim_state.mode == VimMode::Insert {
                    if self.active_pane == Pane::QueryEditor {
                        self.query_editor.insert_char(c);
                    } else if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                        self.results_viewer.edit_insert_char(c);
                    } else if self.active_pane == Pane::Results && self.results_viewer.insert_mode {
                        self.results_viewer.edit_insert_char(c);
                    }
                }
            }
            VimCommand::InsertNewline => {
                if self.vim_state.mode == VimMode::Insert && self.active_pane == Pane::QueryEditor
                {
                    self.query_editor.insert_newline();
                }
            }
            VimCommand::Backspace => {
                if self.vim_state.mode == VimMode::Insert {
                    if self.active_pane == Pane::QueryEditor {
                        self.query_editor.backspace();
                    } else if self.active_pane == Pane::Results && self.results_viewer.edit_mode {
                        self.results_viewer.edit_backspace();
                    } else if self.active_pane == Pane::Results && self.results_viewer.insert_mode {
                        self.results_viewer.edit_backspace();
                    }
                }
            }
            VimCommand::DeleteChar => {
                if self.active_pane == Pane::QueryEditor {
                    self.query_editor.delete_char();
                }
            }
            VimCommand::YankSelection => {
                if self.active_pane == Pane::QueryEditor {
                    if let Some(text) = self.query_editor.get_selection() {
                        self.clipboard = Some(text);
                    }
                    self.query_editor.exit_visual_mode();
                }
            }
            VimCommand::DeleteConnection => {
                if self.active_pane == Pane::DatabaseBrowser {
                    self.delete_connection()?;
                }
            }
            VimCommand::OpenConnectionManager => {
                self.connection_manager.show();
            }
            VimCommand::CloseConnectionManager => {
                self.connection_manager.hide();
            }
            VimCommand::ConnectionManagerAction(action) => {
                self.handle_connection_manager_action(action)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_command(&mut self, cmd: &str) -> Result<()> {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "q" | "quit" => {
                self.should_quit = true;
            }
            "open" => {
                if parts.len() < 2 {
                    return Ok(());
                }
                let path = parts[1..].join(" ");
                self.open_database(&path, DatabaseType::SQLite)?;
            }
            "mysql" => {
                if parts.len() < 2 {
                    return Ok(());
                }
                let connection_string = parts[1..].join(" ");
                self.open_database(&connection_string, DatabaseType::MySQL)?;
            }
            "mariadb" => {
                if parts.len() < 2 {
                    return Ok(());
                }
                let connection_string = parts[1..].join(" ");
                self.open_database(&connection_string, DatabaseType::MariaDB)?;
            }
            "exec" | "execute" => {
                self.execute_query()?;
            }
            "clear" => {
                self.query_editor.clear();
                self.results_viewer.clear();
            }
            "disconnect" | "close" => {
                self.delete_connection()?;
            }
            "connections" | "conn" => {
                self.connection_manager.show();
            }
            _ => {}
        }
        Ok(())
    }

    fn open_database(&mut self, connection_string: &str, db_type: DatabaseType) -> Result<()> {
        // Create connection based on database type
        let mut conn: Box<dyn DatabaseConnection> = match db_type {
            DatabaseType::SQLite => SQLiteConnection::connect(connection_string)?,
            DatabaseType::MySQL | DatabaseType::MariaDB => MySQLConnection::connect(connection_string)?,
        };

        let id = self.next_connection_id;
        self.next_connection_id += 1;

        // Generate connection name
        let name = match db_type {
            DatabaseType::SQLite => {
                std::path::Path::new(connection_string)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(connection_string)
                    .to_string()
            },
            DatabaseType::MySQL => {
                // Extract database name from connection string if possible
                if let Some(db_name) = connection_string.split('/').last() {
                    format!("MySQL: {}", db_name.split('?').next().unwrap_or(db_name))
                } else {
                    format!("MySQL Connection {}", id)
                }
            },
            DatabaseType::MariaDB => {
                if let Some(db_name) = connection_string.split('/').last() {
                    format!("MariaDB: {}", db_name.split('?').next().unwrap_or(db_name))
                } else {
                    format!("MariaDB Connection {}", id)
                }
            },
        };

        let conn_info = ConnectionInfo {
            id,
            name: name.clone(),
            db_type: db_type.clone(),
            connection_string: connection_string.to_string(),
        };

        self.database_browser.add_connection(conn_info);
        self.database_browser.selected_connection = Some(id);

        // Load tables
        let tables = conn.list_tables()?;
        self.database_browser.set_tables(tables);

        self.connections.insert(id, conn);

        // Save to config
        let db_type_str = match db_type {
            DatabaseType::SQLite => "sqlite".to_string(),
            DatabaseType::MySQL => "mysql".to_string(),
            DatabaseType::MariaDB => "mariadb".to_string(),
        };
        self.config.add_connection(name, connection_string.to_string(), db_type_str);
        self.config.save()?;

        Ok(())
    }

    fn execute_query(&mut self) -> Result<()> {
        let query = self.query_editor.get_query();
        if query.trim().is_empty() {
            return Ok(());
        }

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let result = conn.execute_query(&query)?;
                self.results_viewer.set_result(result);
                self.active_pane = Pane::Results;
                self.update_focus();
            }
        }

        Ok(())
    }

    fn execute_query_at_cursor(&mut self) -> Result<()> {
        let query = self.query_editor.get_query_at_cursor();
        if query.trim().is_empty() {
            return Ok(());
        }

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let result = conn.execute_query(&query)?;
                self.results_viewer.set_result(result);
                self.active_pane = Pane::Results;
                self.update_focus();
            }
        }

        Ok(())
    }

    fn load_selected_table_data(&mut self) -> Result<()> {
        // Get selected table
        let table_name = match self.database_browser.get_selected_table() {
            Some(table) => table.name.clone(),
            None => return Ok(()), // No table selected
        };

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                // Load first 1000 rows from the table
                let result = conn.get_table_data(&table_name, 1000, 0)?;
                self.results_viewer.set_result(result);
                self.results_viewer.set_table_name(table_name);
                self.active_pane = Pane::Results;
                self.update_focus();
            }
        }

        Ok(())
    }

    fn save_table_edits(&mut self) -> Result<()> {
        if !self.results_viewer.has_modifications() {
            return Ok(());
        }

        // Generate UPDATE queries
        let queries = self.results_viewer.generate_update_queries();

        if queries.is_empty() {
            return Ok(());
        }

        // Execute each UPDATE query
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                for query in &queries {
                    conn.execute_query(query)?;
                }

                // Clear modifications after successful save
                self.results_viewer.clear_modifications();

                // Reload the table data to show updated values
                if let Some(table_name) = &self.results_viewer.table_name.clone() {
                    let result = conn.get_table_data(table_name, 1000, 0)?;
                    self.results_viewer.set_result(result);
                    self.results_viewer.set_table_name(table_name.clone());
                }
            }
        }

        Ok(())
    }

    fn save_insert_row(&mut self) -> Result<()> {
        if !self.results_viewer.has_insert_data() {
            return Ok(());
        }

        // Save the current field before generating query
        self.results_viewer.save_insert_field();

        // Generate INSERT query
        let query = match self.results_viewer.generate_insert_query() {
            Some(q) => q,
            None => return Ok(()),
        };

        // Execute INSERT query
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.execute_query(&query)?;

                // Clear insert data after successful insert
                self.results_viewer.clear_insert_data();
                self.results_viewer.exit_insert_mode();
                self.vim_state.enter_normal_mode();

                // Reload the table data to show the new row
                if let Some(table_name) = &self.results_viewer.table_name.clone() {
                    let result = conn.get_table_data(table_name, 1000, 0)?;
                    self.results_viewer.set_result(result);
                    self.results_viewer.set_table_name(table_name.clone());
                }
            }
        }

        Ok(())
    }

    fn load_saved_connections(&mut self) -> Result<()> {
        let connections = self.config.get_connections().to_vec();

        for conn_config in connections {
            // Determine database type
            let db_type = match conn_config.db_type.as_str() {
                "mysql" => DatabaseType::MySQL,
                "mariadb" => DatabaseType::MariaDB,
                _ => DatabaseType::SQLite, // Default to SQLite
            };

            // Try to open each saved connection
            if let Ok(()) = self.open_database(&conn_config.connection_string, db_type) {
                // Connection opened successfully
            } else {
                // If connection fails, we can just skip it
                // The user can manually remove it later with X
            }
        }

        Ok(())
    }

    fn handle_connection_manager_action(&mut self, action: char) -> Result<()> {
        use crate::ui::connection_manager::ConnectionManagerMode;

        match self.connection_manager.mode {
            ConnectionManagerMode::List => {
                match action {
                    'n' => {
                        // New connection
                        self.connection_manager.show_add_form();
                    }
                    'e' => {
                        // Edit connection
                        if let Some(selected) = self.connection_manager.list_state.selected() {
                            let connections = self.config.get_connections();
                            if let Some(conn) = connections.get(selected) {
                                let db_type = match conn.db_type.as_str() {
                                    "mysql" => DatabaseType::MySQL,
                                    "mariadb" => DatabaseType::MariaDB,
                                    _ => DatabaseType::SQLite,
                                };
                                self.connection_manager.show_edit_form(
                                    selected,
                                    conn.name.clone(),
                                    db_type,
                                    conn.connection_string.clone(),
                                );
                            }
                        }
                    }
                    'd' => {
                        // Delete connection
                        if let Some(selected) = self.connection_manager.list_state.selected() {
                            let conn_name = {
                                let connections = self.config.get_connections();
                                connections.get(selected).map(|c| c.name.clone())
                            };
                            if let Some(name) = conn_name {
                                self.config.remove_connection(&name);
                                self.config.save()?;
                                // Move selection if needed
                                let conn_count = self.config.get_connections().len();
                                if selected >= conn_count && conn_count > 0 {
                                    self.connection_manager.list_state.select(Some(conn_count - 1));
                                }
                            }
                        }
                    }
                    't' => {
                        // Test connection
                        if let Some(selected) = self.connection_manager.list_state.selected() {
                            let test_data = {
                                let connections = self.config.get_connections();
                                connections.get(selected).map(|conn| {
                                    let db_type = match conn.db_type.as_str() {
                                        "mysql" => DatabaseType::MySQL,
                                        "mariadb" => DatabaseType::MariaDB,
                                        _ => DatabaseType::SQLite,
                                    };
                                    (conn.connection_string.clone(), db_type)
                                })
                            };
                            if let Some((conn_str, db_type)) = test_data {
                                self.test_connection(&conn_str, db_type)?;
                            }
                        }
                    }
                    '\n' => {
                        // Connect to selected
                        if let Some(selected) = self.connection_manager.list_state.selected() {
                            let connect_data = {
                                let connections = self.config.get_connections();
                                connections.get(selected).map(|conn| {
                                    let db_type = match conn.db_type.as_str() {
                                        "mysql" => DatabaseType::MySQL,
                                        "mariadb" => DatabaseType::MariaDB,
                                        _ => DatabaseType::SQLite,
                                    };
                                    (conn.connection_string.clone(), db_type)
                                })
                            };
                            if let Some((conn_str, db_type)) = connect_data {
                                self.open_database(&conn_str, db_type)?;
                            }
                        }
                        self.connection_manager.hide();
                    }
                    _ => {}
                }
            }
            ConnectionManagerMode::Add | ConnectionManagerMode::Edit(_) => {
                // Forms are handled in the key_event handler directly
            }
            _ => {}
        }

        Ok(())
    }

    fn save_connection_from_form(&mut self) -> Result<()> {
        use crate::ui::connection_manager::ConnectionManagerMode;

        if self.connection_manager.form.name.is_empty() {
            self.connection_manager.test_result = Some("✗ Error: Name is required".to_string());
            return Ok(());
        }

        if self.connection_manager.form.connection_string.is_empty() {
            self.connection_manager.test_result = Some("✗ Error: Connection string is required".to_string());
            return Ok(());
        }

        let db_type_str = match self.connection_manager.form.db_type {
            DatabaseType::SQLite => "sqlite".to_string(),
            DatabaseType::MySQL => "mysql".to_string(),
            DatabaseType::MariaDB => "mariadb".to_string(),
        };

        match &self.connection_manager.mode {
            ConnectionManagerMode::Add => {
                // Add new connection
                self.config.add_connection(
                    self.connection_manager.form.name.clone(),
                    self.connection_manager.form.connection_string.clone(),
                    db_type_str,
                );
                self.config.save()?;
                self.connection_manager.mode = ConnectionManagerMode::List;
                self.connection_manager.test_result = None;
            }
            ConnectionManagerMode::Edit(index) => {
                // Remove old and add updated
                let old_conn_name = {
                    let connections = self.config.get_connections();
                    connections.get(*index).map(|c| c.name.clone())
                };
                if let Some(name) = old_conn_name {
                    self.config.remove_connection(&name);
                }
                self.config.add_connection(
                    self.connection_manager.form.name.clone(),
                    self.connection_manager.form.connection_string.clone(),
                    db_type_str,
                );
                self.config.save()?;
                self.connection_manager.mode = ConnectionManagerMode::List;
                self.connection_manager.test_result = None;
            }
            _ => {}
        }

        Ok(())
    }

    fn test_connection(&mut self, connection_string: &str, db_type: DatabaseType) -> Result<()> {
        let result: Result<Box<dyn DatabaseConnection>> = match db_type {
            DatabaseType::SQLite => SQLiteConnection::connect(connection_string).map(|c| c as Box<dyn DatabaseConnection>),
            DatabaseType::MySQL | DatabaseType::MariaDB => MySQLConnection::connect(connection_string),
        };

        match result {
            Ok(_) => {
                self.connection_manager.test_result = Some("✓ Success: Connection successful!".to_string());
            }
            Err(e) => {
                self.connection_manager.test_result = Some(format!("✗ Error: {}", e));
            }
        }

        Ok(())
    }

    fn delete_connection(&mut self) -> Result<()> {
        // Get the selected connection from database browser
        let conn_info = match self.database_browser.get_selected_connection() {
            Some(info) => info.clone(),
            None => return Ok(()), // No connection selected
        };

        let conn_id = conn_info.id;
        let conn_name = conn_info.name.clone();

        // Remove from connections HashMap
        self.connections.remove(&conn_id);

        // Remove from database browser
        self.database_browser.remove_connection(conn_id);

        // Remove from config and save
        self.config.remove_connection(&conn_name);
        self.config.save()?;

        // Clear results viewer if it was using this connection
        self.results_viewer.clear();
        self.results_viewer.table_name = None;

        Ok(())
    }

    fn next_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::DatabaseBrowser => Pane::QueryEditor,
            Pane::QueryEditor => Pane::Results,
            Pane::Results => Pane::DatabaseBrowser,
        };
        self.update_focus();
    }

    fn prev_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::DatabaseBrowser => Pane::Results,
            Pane::QueryEditor => Pane::DatabaseBrowser,
            Pane::Results => Pane::QueryEditor,
        };
        self.update_focus();
    }

    fn update_focus(&mut self) {
        self.database_browser.focused = self.active_pane == Pane::DatabaseBrowser;
        self.query_editor.focused = self.active_pane == Pane::QueryEditor;
        self.results_viewer.focused = self.active_pane == Pane::Results;
    }
}

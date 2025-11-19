use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyEvent};
use std::collections::HashMap;
use std::time::Duration;

#[cfg(target_os = "linux")]
use arboard::SetExtLinux;

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
    pub system_clipboard: Option<Clipboard>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load().unwrap_or_default();

        // Initialize system clipboard - keep it alive for the duration of the app
        let system_clipboard = Clipboard::new().ok();

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
            system_clipboard,
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
                        let conn_str = self.connection_manager.get_connection_string();
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

        // Handle database browser search mode
        if self.active_pane == Pane::DatabaseBrowser && self.database_browser.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.database_browser.exit_search_mode();
                    return Ok(());
                }
                KeyCode::Enter => {
                    self.database_browser.exit_search_mode();
                    return Ok(());
                }
                KeyCode::Backspace => {
                    self.database_browser.search_backspace();
                    return Ok(());
                }
                // Allow navigation keys (j/k, up/down) while in search mode
                KeyCode::Char('j') | KeyCode::Down => {
                    self.database_browser.move_down();
                    return Ok(());
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.database_browser.move_up();
                    return Ok(());
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.database_browser.search_insert_char(c);
                    return Ok(());
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle Escape key for database browser navigation
        if key.code == KeyCode::Esc && self.active_pane == Pane::DatabaseBrowser {
            // Clear search first if active
            if !self.database_browser.search_query.is_empty() {
                self.database_browser.clear_search();
                return Ok(());
            }
            // Then handle going back to database list
            if self.database_browser.is_viewing_tables() {
                // Go back to database list
                self.go_back_to_database_list()?;
                return Ok(());
            }
        }

        // Handle tab switching in Results pane (1, 2, 3 keys)
        if self.active_pane == Pane::Results && !self.results_viewer.edit_mode && !self.results_viewer.insert_mode && !self.results_viewer.schema_edit_mode && !self.results_viewer.schema_insert_mode {
            match key.code {
                KeyCode::Char('1') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.results_viewer.switch_to_data_tab();
                    return Ok(());
                }
                KeyCode::Char('2') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.results_viewer.switch_to_schema_tab();
                    return Ok(());
                }
                KeyCode::Char('3') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.results_viewer.switch_to_indexes_tab();
                    return Ok(());
                }
                _ => {}
            }
        }

        // Handle Ctrl+D for discard changes
        if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL)
            && self.active_pane == Pane::Results && self.results_viewer.has_any_changes() {
            if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                self.results_viewer.discard_schema_changes();
            } else {
                self.results_viewer.discard_all_changes();
            }
            self.vim_state.enter_normal_mode();
            return Ok(());
        }

        // Handle schema tab edit/insert mode key input
        if self.active_pane == Pane::Results && self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema
            && (self.results_viewer.schema_edit_mode || self.results_viewer.schema_insert_mode) {
            match key.code {
                KeyCode::Esc => {
                    if self.results_viewer.schema_edit_mode {
                        self.results_viewer.exit_schema_edit_mode();
                    } else {
                        self.results_viewer.exit_schema_insert_mode();
                    }
                    return Ok(());
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.results_viewer.schema_move_column_left();
                    return Ok(());
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.results_viewer.schema_move_column_right();
                    return Ok(());
                }
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Save schema changes
                    if self.results_viewer.schema_insert_mode {
                        self.save_schema_insert_column()?;
                    } else {
                        self.save_schema_edits()?;
                    }
                    return Ok(());
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.results_viewer.schema_insert_char(c);
                    return Ok(());
                }
                KeyCode::Backspace => {
                    self.results_viewer.schema_backspace();
                    return Ok(());
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
                    if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                        self.results_viewer.enter_schema_insert_mode();
                        self.vim_state.enter_insert_mode();
                    } else {
                        self.results_viewer.enter_insert_mode();
                        self.vim_state.enter_insert_mode();
                    }
                }
            }
            VimCommand::SaveInsertRow => {
                if self.active_pane == Pane::Results {
                    self.save_insert_row()?;
                }
            }
            VimCommand::EnterEditMode => {
                if self.active_pane == Pane::Results {
                    if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                        self.results_viewer.enter_schema_edit_mode();
                        self.vim_state.enter_insert_mode();
                    } else {
                        self.results_viewer.enter_edit_mode();
                        self.vim_state.enter_insert_mode();
                    }
                }
            }
            VimCommand::ExitEditMode => {
                if self.active_pane == Pane::Results {
                    if self.results_viewer.schema_edit_mode {
                        self.results_viewer.exit_schema_edit_mode();
                        self.vim_state.enter_normal_mode();
                    } else if self.results_viewer.edit_mode {
                        self.results_viewer.exit_edit_mode();
                        self.vim_state.enter_normal_mode();
                    }
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
                    if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                        for _ in 0..count {
                            self.results_viewer.schema_move_up();
                        }
                    } else {
                        self.results_viewer.move_up(count);
                    }
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
                    if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                        for _ in 0..count {
                            self.results_viewer.schema_move_down();
                        }
                    } else {
                        self.results_viewer.move_down(count);
                    }
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
                } else if self.active_pane == Pane::Results {
                    // Move selected column left when not in edit mode
                    for _ in 0..count {
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
                } else if self.active_pane == Pane::Results {
                    // Move selected column right when not in edit mode
                    for _ in 0..count {
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
                    } else if self.active_pane == Pane::Results && (self.results_viewer.edit_mode || self.results_viewer.insert_mode) {
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
                    } else if self.active_pane == Pane::Results && (self.results_viewer.edit_mode || self.results_viewer.insert_mode) {
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
            VimCommand::StartSearch => {
                if self.active_pane == Pane::DatabaseBrowser {
                    self.database_browser.enter_search_mode();
                }
            }
            VimCommand::DiscardChanges => {
                if self.active_pane == Pane::Results {
                    if self.results_viewer.active_tab == crate::ui::results_viewer::TabMode::Schema {
                        self.results_viewer.discard_schema_changes();
                    } else {
                        self.results_viewer.discard_all_changes();
                    }
                    self.vim_state.enter_normal_mode();
                }
            }
            VimCommand::RefreshData => {
                if self.active_pane == Pane::Results {
                    self.refresh_current_table()?;
                } else if self.active_pane == Pane::DatabaseBrowser {
                    // Refresh database/table list
                    if let Some(conn_id) = self.database_browser.selected_connection {
                        if let Some(conn) = self.connections.get_mut(&conn_id) {
                            let tables = conn.list_tables()?;
                            self.database_browser.set_tables(tables);
                        }
                    }
                }
            }
            VimCommand::CopyCellValue => {
                if self.active_pane == Pane::Results {
                    if let Some(value) = self.results_viewer.get_current_cell_value() {
                        // Copy to internal clipboard
                        self.clipboard = Some(value.clone());

                        // Copy to system clipboard using the persistent instance
                        if let Some(clipboard) = &mut self.system_clipboard {
                            #[cfg(target_os = "linux")]
                            let result = clipboard.set()
                                .wait()
                                .text(&value);

                            #[cfg(not(target_os = "linux"))]
                            let result = clipboard.set_text(&value);

                            match result {
                                Ok(_) => {
                                    self.results_viewer.set_status_message(format!("Copied to clipboard: {}",
                                        if value.len() > 50 {
                                            format!("{}...", &value[..50])
                                        } else {
                                            value
                                        }));
                                }
                                Err(e) => {
                                    self.results_viewer.set_status_message(format!("Copy failed: {}", e));
                                }
                            }
                        } else {
                            self.results_viewer.set_status_message("Clipboard not available".to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_command(&mut self, cmd: &str) -> Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
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
        self.open_database_with_save(connection_string, db_type, true)
    }

    fn open_database_no_save(&mut self, connection_string: &str, db_type: DatabaseType) -> Result<()> {
        self.open_database_with_save(connection_string, db_type, false)
    }

    fn open_database_with_save(&mut self, connection_string: &str, db_type: DatabaseType, save_to_config: bool) -> Result<()> {
        // If a connection with the same connection string already exists in the UI,
        // reuse it instead of adding a duplicate entry. If the UI knows about it
        // but we haven't opened the actual connection object yet, open and insert it.
        if let Some(existing) = self.database_browser.connections.iter().find(|c| c.connection_string == connection_string) {
            // Select existing connection
            let id = existing.id;
            self.database_browser.selected_connection = Some(id);

            // If connection object isn't opened yet, open and insert it
            if !self.connections.contains_key(&id) {
                let mut conn: Box<dyn DatabaseConnection> = match db_type {
                    DatabaseType::SQLite => SQLiteConnection::connect(connection_string)?,
                    DatabaseType::MySQL | DatabaseType::MariaDB => MySQLConnection::connect(connection_string)?,
                };

                // Load tables for the selected connection
                let tables = conn.list_tables()?;
                self.database_browser.set_tables(tables);
                self.connections.insert(id, conn);
            }

            return Ok(());
        }

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
                if let Some(db_name) = connection_string.split('/').next_back() {
                    format!("MySQL: {}", db_name.split('?').next().unwrap_or(db_name))
                } else {
                    format!("MySQL Connection {}", id)
                }
            },
            DatabaseType::MariaDB => {
                if let Some(db_name) = connection_string.split('/').next_back() {
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

        // Save to config only if requested
        if save_to_config {
            let db_type_str = match db_type {
                DatabaseType::SQLite => "sqlite".to_string(),
                DatabaseType::MySQL => "mysql".to_string(),
                DatabaseType::MariaDB => "mariadb".to_string(),
            };
            self.config.add_connection(name, connection_string.to_string(), db_type_str);
            self.config.save()?;
        }

        Ok(())
    }

    fn go_back_to_database_list(&mut self) -> Result<()> {
        // Reset the database browser to show databases instead of tables
        self.database_browser.go_back_to_databases();
        
        // For MySQL/MariaDB connections, clear database context and reload database list
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let conn_info = self.database_browser.connections
                    .iter()
                    .find(|c| c.id == conn_id);
                
                if let Some(info) = conn_info {
                    if matches!(info.db_type, crate::db::DatabaseType::MySQL | crate::db::DatabaseType::MariaDB) {
                        // For MySQL, clear the database context first, then get database list
                        if let Some(mysql_conn) = conn.as_any_mut().downcast_mut::<crate::db::mysql::MySQLConnection>() {
                            mysql_conn.clear_database_context()?;
                        }
                        let tables = conn.list_tables()?;
                        self.database_browser.set_tables(tables);
                    }
                }
            }
        }
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
        // Get selected item (could be table or database)
        let selected_name = match self.database_browser.get_selected_table() {
            Some(item) => item.name.clone(),
            None => return Ok(()), // No item selected
        };

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                // First, check if this is a MySQL connection and we might be selecting a database
                let conn_info = self.database_browser.connections
                    .iter()
                    .find(|c| c.id == conn_id);
                
                if let Some(info) = conn_info {
                    if matches!(info.db_type, crate::db::DatabaseType::MySQL | crate::db::DatabaseType::MariaDB) {
                        // For MySQL connections, check if we're looking at databases or tables
                        // If no table has row_count data, we're probably looking at databases
                        let is_showing_databases = self.database_browser.tables.iter()
                            .all(|t| t.row_count.is_none());
                        
                        if is_showing_databases {
                            // This is a database selection, switch to it
                            if let Some(mysql_conn) = conn.as_any_mut().downcast_mut::<crate::db::mysql::MySQLConnection>() {
                                mysql_conn.use_database(&selected_name)?;
                                // Store the current database in the browser
                                self.database_browser.set_current_database(Some(selected_name.clone()));
                                // Reload tables for the selected database
                                let tables = conn.list_tables()?;
                                self.database_browser.set_tables(tables);
                                return Ok(());
                            }
                        } else {
                            // This is a table selection for MySQL, ensure database context
                            if let Some(mysql_conn) = conn.as_any_mut().downcast_mut::<crate::db::mysql::MySQLConnection>() {
                                // Ensure we're using the correct database
                                if let Some(db_name) = self.database_browser.get_current_database() {
                                    mysql_conn.use_database(db_name)?;
                                }
                            }
                        }
                    }
                }
                
                // This is a table selection, load table data
                let result = conn.get_table_data(&selected_name, 1000, 0)?;

                self.results_viewer.set_result(result);
                self.results_viewer.set_table_name(selected_name.clone());

                // Load schema information (DDL)
                let schema = Self::get_table_schema(conn, &selected_name)?;
                self.results_viewer.set_schema_info(schema);

                // Load column information for schema table
                let columns = Self::get_column_info(conn, &selected_name)?;
                self.results_viewer.set_schema_columns(columns);

                // Load index information
                let indexes = Self::get_table_indexes(conn, &selected_name)?;
                self.results_viewer.set_indexes_info(indexes);

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
                                
                                // Use detailed form if we have individual components stored
                                if conn.username.is_some() || conn.password.is_some() || conn.host.is_some() || conn.port.is_some() || conn.database.is_some() {
                                    self.connection_manager.show_edit_form_detailed(
                                        selected,
                                        conn.name.clone(),
                                        db_type,
                                        conn.connection_string.clone(),
                                        conn.username.clone(),
                                        conn.password.clone(),
                                        conn.host.clone(),
                                        conn.port.clone(),
                                        conn.database.clone(),
                                    );
                                } else {
                                    // Fallback to parsing connection string
                                    self.connection_manager.show_edit_form(
                                        selected,
                                        conn.name.clone(),
                                        db_type,
                                        conn.connection_string.clone(),
                                    );
                                }
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
                                    
                                    // For MySQL/MariaDB, rebuild connection string if we have individual components
                                    let connection_string = if matches!(db_type, DatabaseType::MySQL | DatabaseType::MariaDB) &&
                                        (conn.username.is_some() || conn.host.is_some()) {
                                        self.build_connection_string_from_config(conn, &db_type)
                                    } else {
                                        conn.connection_string.clone()
                                    };
                                    
                                    (connection_string, db_type)
                                })
                            };
                            if let Some((conn_str, db_type)) = connect_data {
                                match self.open_database_no_save(&conn_str, db_type) {
                                    Ok(()) => {
                                        self.connection_manager.hide();
                                    }
                                    Err(e) => {
                                        self.connection_manager.test_result = Some(format!("✗ Connection failed: {}", e));
                                        // Don't hide the connection manager so user can see the error
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        // Only hide if we get here without errors
                        if self.connection_manager.test_result.is_none() {
                            self.connection_manager.hide();
                        }
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

        // Validate based on database type
        match self.connection_manager.form.db_type {
            DatabaseType::SQLite => {
                if self.connection_manager.form.connection_string.is_empty() {
                    self.connection_manager.test_result = Some("✗ Error: File path is required".to_string());
                    return Ok(());
                }
            }
            DatabaseType::MySQL | DatabaseType::MariaDB => {
                if self.connection_manager.form.username.is_empty() {
                    self.connection_manager.test_result = Some("✗ Error: Username is required".to_string());
                    return Ok(());
                }
                if self.connection_manager.form.host.is_empty() {
                    self.connection_manager.test_result = Some("✗ Error: Host is required".to_string());
                    return Ok(());
                }
                if self.connection_manager.form.port.is_empty() {
                    self.connection_manager.test_result = Some("✗ Error: Port is required".to_string());
                    return Ok(());
                }
            }
        }

        let db_type_str = match self.connection_manager.form.db_type {
            DatabaseType::SQLite => "sqlite".to_string(),
            DatabaseType::MySQL => "mysql".to_string(),
            DatabaseType::MariaDB => "mariadb".to_string(),
        };

        // Get the final connection string
        let connection_string = self.connection_manager.get_connection_string();

        // Prepare optional fields for MySQL/MariaDB
        let (username, password, host, port, database) = match self.connection_manager.form.db_type {
            DatabaseType::SQLite => (None, None, None, None, None),
            DatabaseType::MySQL | DatabaseType::MariaDB => (
                if self.connection_manager.form.username.is_empty() { None } else { Some(self.connection_manager.form.username.clone()) },
                if self.connection_manager.form.password.is_empty() { None } else { Some(self.connection_manager.form.password.clone()) },
                if self.connection_manager.form.host.is_empty() { None } else { Some(self.connection_manager.form.host.clone()) },
                if self.connection_manager.form.port.is_empty() { None } else { Some(self.connection_manager.form.port.clone()) },
                if self.connection_manager.form.database.is_empty() { None } else { Some(self.connection_manager.form.database.clone()) },
            ),
        };

        match &self.connection_manager.mode {
            ConnectionManagerMode::Add => {
                // Add new connection
                self.config.add_connection_detailed(
                    self.connection_manager.form.name.clone(),
                    connection_string,
                    db_type_str,
                    username,
                    password,
                    host,
                    port,
                    database,
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
                self.config.add_connection_detailed(
                    self.connection_manager.form.name.clone(),
                    connection_string,
                    db_type_str,
                    username,
                    password,
                    host,
                    port,
                    database,
                );
                self.config.save()?;
                self.connection_manager.mode = ConnectionManagerMode::List;
                self.connection_manager.test_result = None;
            }
            _ => {}
        }

        Ok(())
    }

    fn build_connection_string_from_config(&self, conn: &crate::config::ConnectionConfig, db_type: &DatabaseType) -> String {
        match db_type {
            DatabaseType::SQLite => conn.connection_string.clone(),
            DatabaseType::MySQL | DatabaseType::MariaDB => {
                let protocol = match db_type {
                    DatabaseType::MySQL => "mysql",
                    DatabaseType::MariaDB => "mysql", // MariaDB uses mysql protocol
                    _ => unreachable!(),
                };
                
                let mut url = format!("{}://", protocol);
                
                // Add username and password if provided
                if let Some(username) = &conn.username {
                    url.push_str(username);
                    if let Some(password) = &conn.password {
                        url.push(':');
                        url.push_str(password);
                    }
                    url.push('@');
                }
                
                // Add host
                let host = conn.host.as_deref().unwrap_or("localhost");
                url.push_str(host);
                
                // Add port if not default
                let port = conn.port.as_deref().unwrap_or("3306");
                if port != "3306" {
                    url.push(':');
                    url.push_str(port);
                }
                
                // Add database if provided
                if let Some(database) = &conn.database {
                    if !database.is_empty() {
                        url.push('/');
                        url.push_str(database);
                    }
                }
                
                url
            }
        }
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

    fn get_column_info(conn: &mut Box<dyn DatabaseConnection>, table_name: &str) -> Result<Vec<crate::ui::results_viewer::ColumnInfo>> {
        use crate::ui::results_viewer::ColumnInfo;
        let mut columns = Vec::new();

        // Try SQLite PRAGMA first
        let pragma_query = format!("PRAGMA table_info({})", table_name);
        match conn.execute_query(&pragma_query) {
            Ok(result) => {
                // SQLite PRAGMA table_info returns: cid, name, type, notnull, dflt_value, pk
                for row in &result.rows {
                    if row.len() >= 6 {
                        let name = row[1].clone();
                        let data_type = row[2].clone();
                        let not_null = &row[3];
                        let default_value = row[4].clone();
                        let is_pk = &row[5];

                        let nullable = if not_null == "0" { "YES" } else { "NO" };
                        let extra = if is_pk != "0" { "PRIMARY KEY" } else { "" };

                        columns.push(ColumnInfo {
                            name,
                            data_type,
                            nullable: nullable.to_string(),
                            default_value,
                            extra: extra.to_string(),
                        });
                    }
                }
                return Ok(columns);
            }
            Err(_) => {
                // Try MySQL/MariaDB DESCRIBE
                let describe_query = format!("DESCRIBE {}", table_name);
                if let Ok(result) = conn.execute_query(&describe_query) {
                    // DESCRIBE returns: Field, Type, Null, Key, Default, Extra
                    for row in &result.rows {
                        if row.len() >= 6 {
                            let name = row[0].clone();
                            let data_type = row[1].clone();
                            let nullable = row[2].clone();
                            let key = &row[3];
                            let default_value = row[4].clone();
                            let extra_val = row[5].clone();

                            let mut extra = extra_val.clone();
                            if key == "PRI" {
                                extra = if extra.is_empty() {
                                    "PRIMARY KEY".to_string()
                                } else {
                                    format!("PRIMARY KEY, {}", extra)
                                };
                            }

                            columns.push(ColumnInfo {
                                name,
                                data_type,
                                nullable,
                                default_value,
                                extra,
                            });
                        }
                    }
                    return Ok(columns);
                }
            }
        }

        Ok(columns)
    }

    fn get_table_schema(conn: &mut Box<dyn DatabaseConnection>, table_name: &str) -> Result<String> {
        // Try to get CREATE TABLE statement
        let query = format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{}'", table_name);

        match conn.execute_query(&query) {
            Ok(result) => {
                if !result.rows.is_empty() && !result.rows[0].is_empty() {
                    Ok(result.rows[0][0].clone())
                } else {
                    // Fallback: describe table structure manually
                    let pragma_query = format!("PRAGMA table_info({})", table_name);
                    match conn.execute_query(&pragma_query) {
                        Ok(info_result) => {
                            let mut schema = format!("CREATE TABLE {} (\n", table_name);
                            for (i, row) in info_result.rows.iter().enumerate() {
                                if row.len() >= 3 {
                                    let col_name = &row[1];
                                    let col_type = &row[2];
                                    schema.push_str(&format!("  {} {}", col_name, col_type));
                                    if i < info_result.rows.len() - 1 {
                                        schema.push_str(",\n");
                                    } else {
                                        schema.push('\n');
                                    }
                                }
                            }
                            schema.push_str(");");
                            Ok(schema)
                        }
                        Err(_) => Ok(format!("Table: {}\n\nSchema information not available", table_name)),
                    }
                }
            }
            Err(_) => {
                // For MySQL/MariaDB
                let show_create = format!("SHOW CREATE TABLE {}", table_name);
                match conn.execute_query(&show_create) {
                    Ok(result) => {
                        if !result.rows.is_empty() && result.rows[0].len() >= 2 {
                            Ok(result.rows[0][1].clone())
                        } else {
                            Ok(format!("Table: {}\n\nSchema information not available", table_name))
                        }
                    }
                    Err(_) => Ok(format!("Table: {}\n\nSchema information not available", table_name)),
                }
            }
        }
    }

    fn get_table_indexes(conn: &mut Box<dyn DatabaseConnection>, table_name: &str) -> Result<Vec<String>> {
        let mut indexes = Vec::new();

        // Try SQLite first
        let query = format!("PRAGMA index_list({})", table_name);
        match conn.execute_query(&query) {
            Ok(result) => {
                for row in &result.rows {
                    if !row.is_empty() {
                        let index_name = &row[1];
                        // Get index details
                        let detail_query = format!("PRAGMA index_info({})", index_name);
                        if let Ok(detail_result) = conn.execute_query(&detail_query) {
                            let mut columns = Vec::new();
                            for detail_row in &detail_result.rows {
                                if detail_row.len() >= 3 {
                                    columns.push(detail_row[2].clone());
                                }
                            }
                            indexes.push(format!("Index: {}\nColumns: {}", index_name, columns.join(", ")));
                        }
                    }
                }
                return Ok(indexes);
            }
            Err(_) => {
                // Try MySQL/MariaDB
                let show_indexes = format!("SHOW INDEX FROM {}", table_name);
                if let Ok(result) = conn.execute_query(&show_indexes) {
                    let mut index_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
                    for row in &result.rows {
                        if row.len() >= 5 {
                            let index_name = &row[2];
                            let column_name = &row[4];
                            index_map.entry(index_name.clone()).or_default().push(column_name.clone());
                        }
                    }
                    for (index_name, columns) in index_map {
                        indexes.push(format!("Index: {}\nColumns: {}", index_name, columns.join(", ")));
                    }
                    return Ok(indexes);
                }
            }
        }

        Ok(indexes)
    }

    fn save_schema_edits(&mut self) -> Result<()> {
        // Generate ALTER TABLE statements for modified columns
        let table_name = match &self.results_viewer.table_name {
            Some(name) => name.clone(),
            None => return Ok(()),
        };

        if self.results_viewer.schema_modified_cells.is_empty() {
            return Ok(());
        }

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                // Group modifications by row (column)
                let mut column_changes: std::collections::HashMap<usize, Vec<(usize, String)>> = std::collections::HashMap::new();
                for ((row, col), value) in &self.results_viewer.schema_modified_cells {
                    column_changes.entry(*row).or_default().push((*col, value.clone()));
                }

                for (row_idx, changes) in column_changes {
                    if let Some(column_info) = self.results_viewer.schema_columns.get(row_idx) {
                        let mut new_name = column_info.name.clone();
                        let mut new_type = column_info.data_type.clone();

                        // Apply changes
                        for (col_idx, value) in changes {
                            match col_idx {
                                0 => new_name = value,
                                1 => new_type = value,
                                _ => {} // Other fields not supported for ALTER yet
                            }
                        }

                        // Generate ALTER TABLE statement
                        let alter_query = format!(
                            "ALTER TABLE {} CHANGE COLUMN {} {} {}",
                            table_name,
                            column_info.name,
                            new_name,
                            new_type
                        );

                        conn.execute_query(&alter_query)?;
                    }
                }

                // Clear modifications after successful save
                self.results_viewer.schema_modified_cells.clear();
                self.results_viewer.exit_schema_edit_mode();

                // Reload schema
                let schema = Self::get_table_schema(conn, &table_name)?;
                self.results_viewer.set_schema_info(schema);
                let columns = Self::get_column_info(conn, &table_name)?;
                self.results_viewer.set_schema_columns(columns);
            }
        }

        Ok(())
    }

    fn save_schema_insert_column(&mut self) -> Result<()> {
        // Generate ALTER TABLE ADD COLUMN statement
        let table_name = match &self.results_viewer.table_name {
            Some(name) => name.clone(),
            None => return Ok(()),
        };

        // Save current field first
        self.results_viewer.save_schema_insert_field();

        let column_name = self.results_viewer.schema_insert_row.get(&0).cloned().unwrap_or_default();
        let data_type = self.results_viewer.schema_insert_row.get(&1).cloned().unwrap_or_default();

        if column_name.is_empty() || data_type.is_empty() {
            return Ok(()); // Need at least name and type
        }

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                // Build ALTER TABLE ADD COLUMN statement
                let mut alter_query = format!("ALTER TABLE {} ADD COLUMN {} {}", table_name, column_name, data_type);

                // Add nullable constraint if specified
                if let Some(nullable) = self.results_viewer.schema_insert_row.get(&2) {
                    if nullable.to_uppercase() == "NO" || nullable.to_uppercase() == "NOT NULL" {
                        alter_query.push_str(" NOT NULL");
                    }
                }

                // Add default value if specified
                if let Some(default_val) = self.results_viewer.schema_insert_row.get(&3) {
                    if !default_val.is_empty() {
                        alter_query.push_str(&format!(" DEFAULT {}", default_val));
                    }
                }

                conn.execute_query(&alter_query)?;

                // Clear insert row
                self.results_viewer.exit_schema_insert_mode();

                // Reload schema
                let schema = Self::get_table_schema(conn, &table_name)?;
                self.results_viewer.set_schema_info(schema);
                let columns = Self::get_column_info(conn, &table_name)?;
                self.results_viewer.set_schema_columns(columns);
            }
        }

        Ok(())
    }

    fn refresh_current_table(&mut self) -> Result<()> {
        // Get the current table name
        let table_name = match &self.results_viewer.table_name {
            Some(name) => name.clone(),
            None => return Ok(()), // No table loaded
        };

        // Get active connection
        if let Some(conn_id) = self.database_browser.selected_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                // Reload table data
                let result = conn.get_table_data(&table_name, 1000, 0)?;
                self.results_viewer.set_result(result);

                // Reload schema information
                let schema = Self::get_table_schema(conn, &table_name)?;
                self.results_viewer.set_schema_info(schema);

                // Reload column information for schema table
                let columns = Self::get_column_info(conn, &table_name)?;
                self.results_viewer.set_schema_columns(columns);

                // Reload index information
                let indexes = Self::get_table_indexes(conn, &table_name)?;
                self.results_viewer.set_indexes_info(indexes);

                // Clear any pending changes
                self.results_viewer.discard_all_changes();
                self.results_viewer.discard_schema_changes();
            }
        }

        Ok(())
    }
}

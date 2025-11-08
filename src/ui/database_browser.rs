use crate::db::{ConnectionInfo, TableInfo};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

#[derive(Debug)]
pub struct DatabaseBrowser {
    pub connections: Vec<ConnectionInfo>,
    pub tables: Vec<TableInfo>,
    pub selected_connection: Option<usize>,
    pub selected_table: Option<usize>,
    pub list_state: ListState,
    pub focused: bool,
    pub current_database: Option<String>,
    pub viewing_tables: bool, // true when showing tables, false when showing databases
    pub search_mode: bool,
    pub search_query: String,
}

impl DatabaseBrowser {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));

        Self {
            connections: Vec::new(),
            tables: Vec::new(),
            selected_connection: None,
            selected_table: None,
            list_state: state,
            focused: true,
            current_database: None,
            viewing_tables: false,
            search_mode: false,
            search_query: String::new(),
        }
    }

    pub fn add_connection(&mut self, conn: ConnectionInfo) {
        self.connections.push(conn);
    }

    pub fn set_tables(&mut self, tables: Vec<TableInfo>) {
        let has_tables = !tables.is_empty();
        
        // Determine if we're viewing tables (have row counts) or databases (no row counts)
        self.viewing_tables = has_tables && tables.iter().any(|t| t.row_count.is_some());
        
        self.tables = tables;
        
        if has_tables && self.selected_table.is_none() {
            self.selected_table = Some(0);
            self.list_state.select(Some(0));
        }
    }

    pub fn set_current_database(&mut self, database_name: Option<String>) {
        self.current_database = database_name;
    }

    pub fn get_current_database(&self) -> Option<&str> {
        self.current_database.as_deref()
    }

    pub fn is_viewing_tables(&self) -> bool {
        self.viewing_tables
    }

    pub fn go_back_to_databases(&mut self) {
        self.current_database = None;
        self.viewing_tables = false;
        // Reset selection to first item
        self.list_state.select(Some(0));
    }

    pub fn move_up(&mut self) {
        let total_items = self.get_total_filtered_items();
        if total_items == 0 {
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        let next = if current == 0 {
            total_items - 1
        } else {
            current - 1
        };
        self.list_state.select(Some(next));
        self.update_selection_filtered(next);
    }

    pub fn move_down(&mut self) {
        let total_items = self.get_total_filtered_items();
        if total_items == 0 {
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        let next = if current >= total_items - 1 {
            0
        } else {
            current + 1
        };
        self.list_state.select(Some(next));
        self.update_selection_filtered(next);
    }

    fn update_selection(&mut self, index: usize) {
        if index < self.connections.len() {
            self.selected_connection = Some(index);
            self.selected_table = None;
        } else {
            let table_idx = index - self.connections.len();
            if table_idx < self.tables.len() {
                self.selected_table = Some(table_idx);
            }
        }
    }

    fn update_selection_filtered(&mut self, filtered_index: usize) {
        let (filtered_conns, filtered_tables) = self.get_filtered_items();

        // Handle empty filtered results
        if filtered_conns.is_empty() && filtered_tables.is_empty() {
            return;
        }

        if filtered_index < filtered_conns.len() {
            // Selecting a connection
            let actual_conn_idx = filtered_conns[filtered_index];
            self.selected_connection = Some(actual_conn_idx);
            self.selected_table = None;
        } else if self.selected_connection.is_some() {
            // Selecting a table
            let table_offset = filtered_index - filtered_conns.len();
            if table_offset < filtered_tables.len() {
                let actual_table_idx = filtered_tables[table_offset];
                self.selected_table = Some(actual_table_idx);
            }
        }
    }

    pub fn get_selected_table(&self) -> Option<&TableInfo> {
        self.selected_table.and_then(|idx| self.tables.get(idx))
    }

    pub fn get_selected_connection(&self) -> Option<&ConnectionInfo> {
        self.selected_connection.and_then(|idx| self.connections.get(idx))
    }

    pub fn remove_connection(&mut self, id: usize) -> Option<ConnectionInfo> {
        // Find the connection with the given id
        let pos = self.connections.iter().position(|c| c.id == id)?;
        let removed = self.connections.remove(pos);

        // Clear tables if this was the selected connection
        if self.selected_connection == Some(pos) {
            self.tables.clear();
            self.selected_table = None;
            self.selected_connection = None;
        }

        // Update list selection
        let total_items = self.connections.len() + self.tables.len();
        if total_items > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let next = if current >= total_items { total_items - 1 } else { current };
            self.list_state.select(Some(next));
            self.update_selection(next);
        } else {
            self.list_state.select(Some(0));
        }

        Some(removed)
    }

    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        // Reset selection to first item
        self.list_state.select(Some(0));
    }

    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        // Keep search query active for filtering, just exit input mode
    }

    pub fn search_insert_char(&mut self, c: char) {
        self.search_query.push(c);
        // Reset selection to first filtered item when search changes
        self.list_state.select(Some(0));
        self.update_selection_filtered(0);
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        // Reset selection to first filtered item when search changes
        self.list_state.select(Some(0));
        self.update_selection_filtered(0);
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_mode = false;
        // Reset selection when clearing search
        self.list_state.select(Some(0));
        if !self.connections.is_empty() {
            self.update_selection(0);
        }
    }

    fn get_filtered_items(&self) -> (Vec<usize>, Vec<usize>) {
        let search_lower = self.search_query.to_lowercase();

        // Get filtered connection indices
        let filtered_connections: Vec<usize> = if self.search_query.is_empty() {
            (0..self.connections.len()).collect()
        } else {
            self.connections.iter().enumerate()
                .filter(|(_, conn)| conn.name.to_lowercase().contains(&search_lower))
                .map(|(idx, _)| idx)
                .collect()
        };

        // Get filtered table indices
        let filtered_tables: Vec<usize> = if self.search_query.is_empty() {
            (0..self.tables.len()).collect()
        } else {
            self.tables.iter().enumerate()
                .filter(|(_, table)| table.name.to_lowercase().contains(&search_lower))
                .map(|(idx, _)| idx)
                .collect()
        };

        (filtered_connections, filtered_tables)
    }

    fn get_total_filtered_items(&self) -> usize {
        let (conns, tables) = self.get_filtered_items();
        conns.len() + if self.selected_connection.is_some() { tables.len() } else { 0 }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut items = Vec::new();
        let search_lower = self.search_query.to_lowercase();

        // Add connections
        for (idx, conn) in self.connections.iter().enumerate() {
            // Filter by search query if searching
            if !self.search_query.is_empty() && !conn.name.to_lowercase().contains(&search_lower) {
                continue;
            }

            let icon = if Some(idx) == self.selected_connection {
                "▼ "
            } else {
                "▶ "
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(icon, Style::default().fg(Color::Cyan)),
                Span::styled(&conn.name, Style::default().fg(Color::Green)),
            ])));
        }

        // Add tables if a connection is selected
        if self.selected_connection.is_some() {
            for table in &self.tables {
                // Filter by search query if searching
                if !self.search_query.is_empty() && !table.name.to_lowercase().contains(&search_lower) {
                    continue;
                }

                let count_str = table
                    .row_count
                    .map(|c| format!(" ({})", c))
                    .unwrap_or_default();
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  ├─ "),
                    Span::styled(&table.name, Style::default().fg(Color::Yellow)),
                    Span::styled(count_str, Style::default().fg(Color::DarkGray)),
                ])));
            }
        }

        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Create title with navigation breadcrumbs and search indicator
        let title = if self.search_mode {
            let filtered_count = self.get_total_filtered_items();
            format!(" Search: {} ({} matches) ", self.search_query, filtered_count)
        } else if !self.search_query.is_empty() {
            let filtered_count = self.get_total_filtered_items();
            format!(" Databases (Filtered: {} - {} matches) - Press / to search, ESC to clear ",
                self.search_query, filtered_count)
        } else if self.viewing_tables && self.current_database.is_some() {
            format!(" {} > {} (Press ESC to go back, / to search) ",
                self.connections.get(self.selected_connection.unwrap_or(0))
                    .map(|c| c.name.as_str())
                    .unwrap_or("Connection"),
                self.current_database.as_ref().unwrap())
        } else {
            " Databases (Press / to search) ".to_string()
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}

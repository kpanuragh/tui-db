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
        }
    }

    pub fn add_connection(&mut self, conn: ConnectionInfo) {
        self.connections.push(conn);
    }

    pub fn set_tables(&mut self, tables: Vec<TableInfo>) {
        let has_tables = !tables.is_empty();
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

    pub fn move_up(&mut self) {
        let total_items = self.connections.len() + self.tables.len();
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
        self.update_selection(next);
    }

    pub fn move_down(&mut self) {
        let total_items = self.connections.len() + self.tables.len();
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
        self.update_selection(next);
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

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut items = Vec::new();

        // Add connections
        for (idx, conn) in self.connections.iter().enumerate() {
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

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Databases ")
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

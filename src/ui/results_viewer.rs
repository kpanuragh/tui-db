use crate::db::QueryResult;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState, Paragraph, Tabs},
    text::{Line, Span},
    Frame,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabMode {
    Data,
    Schema,
    Indexes,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: String,
    pub default_value: String,
    pub extra: String, // For auto_increment, etc.
}

#[derive(Debug)]
pub struct ResultsViewer {
    pub result: Option<QueryResult>,
    pub table_state: TableState,
    pub scroll_offset: usize,        // Vertical scroll offset (row)
    pub horizontal_scroll: usize,    // Horizontal scroll offset (column)
    pub focused: bool,
    pub edit_mode: bool,
    pub insert_mode: bool,
    pub selected_column: usize,
    pub modified_cells: HashMap<(usize, usize), String>, // (row, col) -> new value
    pub insert_row: HashMap<usize, String>, // col_idx -> new value for insert
    pub table_name: Option<String>,
    pub edit_buffer: String,
    pub visible_columns: usize,      // Number of columns that can fit in the display
    pub active_tab: TabMode,
    pub schema_info: Option<String>,  // DDL/CREATE statement
    pub indexes_info: Option<Vec<String>>, // List of indexes
    pub schema_columns: Vec<ColumnInfo>, // Parsed column information
    pub schema_table_state: TableState,  // Separate state for schema table
    pub schema_edit_mode: bool,
    pub schema_insert_mode: bool,
    pub schema_selected_column: usize,
    pub schema_edit_buffer: String,
    pub schema_modified_cells: HashMap<(usize, usize), String>, // (row, col) -> new value
    pub schema_insert_row: HashMap<usize, String>, // For new column
    pub status_message: Option<String>, // Temporary status message (e.g., "Copied!")
}

impl ResultsViewer {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));

        let mut schema_state = TableState::default();
        schema_state.select(Some(0));

        Self {
            result: None,
            table_state: state,
            scroll_offset: 0,
            horizontal_scroll: 0,
            focused: false,
            edit_mode: false,
            insert_mode: false,
            selected_column: 0,
            modified_cells: HashMap::new(),
            insert_row: HashMap::new(),
            table_name: None,
            edit_buffer: String::new(),
            visible_columns: 10, // Default to showing 10 columns
            active_tab: TabMode::Data,
            schema_info: None,
            indexes_info: None,
            schema_columns: Vec::new(),
            schema_table_state: schema_state,
            schema_edit_mode: false,
            schema_insert_mode: false,
            schema_selected_column: 0,
            schema_edit_buffer: String::new(),
            schema_modified_cells: HashMap::new(),
            schema_insert_row: HashMap::new(),
            status_message: None,
        }
    }

    pub fn set_result(&mut self, result: QueryResult) {
        self.result = Some(result);
        self.scroll_offset = 0;
        self.horizontal_scroll = 0;
        self.table_state.select(Some(0));
    }

    pub fn clear(&mut self) {
        self.result = None;
        self.scroll_offset = 0;
        self.table_state.select(Some(0));
    }

    pub fn move_up(&mut self, count: usize) {
        self.clear_status_message();
        if let Some(ref result) = self.result {
            if result.rows.is_empty() {
                return;
            }

            let selected = self.table_state.selected().unwrap_or(0);
            let new_selected = selected.saturating_sub(count);
            self.table_state.select(Some(new_selected));
        }
    }

    pub fn move_down(&mut self, count: usize) {
        self.clear_status_message();
        if let Some(ref result) = self.result {
            if result.rows.is_empty() {
                return;
            }

            let selected = self.table_state.selected().unwrap_or(0);
            let new_selected = (selected + count).min(result.rows.len() - 1);
            self.table_state.select(Some(new_selected));
        }
    }

    pub fn goto_top(&mut self) {
        self.table_state.select(Some(0));
        self.scroll_offset = 0;
    }

    pub fn goto_bottom(&mut self) {
        if let Some(ref result) = self.result {
            if !result.rows.is_empty() {
                self.table_state.select(Some(result.rows.len() - 1));
            }
        }
    }

    // Horizontal scrolling methods
    pub fn scroll_left(&mut self) {
        self.clear_status_message();
        if self.horizontal_scroll > 0 {
            self.horizontal_scroll -= 1;
        }
    }

    pub fn scroll_right(&mut self) {
        self.clear_status_message();
        if let Some(ref result) = self.result {
            let max_scroll = result.columns.len().saturating_sub(self.visible_columns);
            if self.horizontal_scroll < max_scroll {
                self.horizontal_scroll += 1;
            }
        }
    }

    pub fn scroll_page_left(&mut self) {
        let scroll_amount = (self.visible_columns / 2).max(1);
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(scroll_amount);
    }

    pub fn scroll_page_right(&mut self) {
        if let Some(ref result) = self.result {
            let max_scroll = result.columns.len().saturating_sub(self.visible_columns);
            let scroll_amount = (self.visible_columns / 2).max(1);
            self.horizontal_scroll = (self.horizontal_scroll + scroll_amount).min(max_scroll);
        }
    }

    pub fn goto_first_column(&mut self) {
        self.horizontal_scroll = 0;
    }

    pub fn goto_last_column(&mut self) {
        if let Some(ref result) = self.result {
            let max_scroll = result.columns.len().saturating_sub(self.visible_columns);
            self.horizontal_scroll = max_scroll;
        }
    }

    pub fn enter_edit_mode(&mut self) {
        self.edit_mode = true;
        self.selected_column = 0;
        // Initialize edit buffer with current cell value
        if let Some(value) = self.get_current_cell_value() {
            self.edit_buffer = value;
        }
    }

    pub fn exit_edit_mode(&mut self) {
        self.edit_mode = false;
        self.edit_buffer.clear();
    }

    pub fn enter_insert_mode(&mut self) {
        self.insert_mode = true;
        self.selected_column = 0;
        self.insert_row.clear();
        self.edit_buffer.clear();
    }

    pub fn exit_insert_mode(&mut self) {
        self.insert_mode = false;
        self.insert_row.clear();
        self.edit_buffer.clear();
    }

    pub fn save_insert_field(&mut self) {
        let col = self.selected_column;
        self.insert_row.insert(col, self.edit_buffer.clone());
    }

    pub fn move_column_left(&mut self) {
        if self.selected_column > 0 {
            self.selected_column -= 1;
            // Auto-scroll if selected column goes off screen to the left
            if self.selected_column < self.horizontal_scroll {
                self.horizontal_scroll = self.selected_column;
            }
            if self.insert_mode {
                // Load insert row value if it exists
                self.edit_buffer = self.insert_row.get(&self.selected_column).cloned().unwrap_or_default();
            } else if let Some(value) = self.get_current_cell_value() {
                self.edit_buffer = value;
            }
        }
    }

    pub fn move_column_right(&mut self) {
        if let Some(ref result) = self.result {
            if self.selected_column < result.columns.len().saturating_sub(1) {
                self.selected_column += 1;
                // Auto-scroll if selected column goes off screen to the right
                let visible_end = self.horizontal_scroll + self.visible_columns;
                if self.selected_column >= visible_end {
                    self.horizontal_scroll = self.selected_column.saturating_sub(self.visible_columns - 1);
                }
                if self.insert_mode {
                    // Load insert row value if it exists
                    self.edit_buffer = self.insert_row.get(&self.selected_column).cloned().unwrap_or_default();
                } else if let Some(value) = self.get_current_cell_value() {
                    self.edit_buffer = value;
                }
            }
        }
    }

    pub fn edit_insert_char(&mut self, c: char) {
        self.edit_buffer.push(c);
    }

    pub fn edit_backspace(&mut self) {
        self.edit_buffer.pop();
    }

    pub fn save_cell_edit(&mut self) {
        let row = self.table_state.selected().unwrap_or(0);
        let col = self.selected_column;
        self.modified_cells.insert((row, col), self.edit_buffer.clone());
    }

    pub fn get_current_cell_value(&self) -> Option<String> {
        let row = self.table_state.selected()?;
        let col = self.selected_column;

        // Check if cell has been modified
        if let Some(modified_value) = self.modified_cells.get(&(row, col)) {
            return Some(modified_value.clone());
        }

        // Otherwise return original value
        self.result.as_ref()?.rows.get(row)?.get(col).cloned()
    }

    pub fn set_table_name(&mut self, name: String) {
        self.table_name = Some(name);
    }

    pub fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub fn has_modifications(&self) -> bool {
        !self.modified_cells.is_empty()
    }

    pub fn generate_update_queries(&self) -> Vec<String> {
        let mut queries = Vec::new();

        let result = match &self.result {
            Some(r) => r,
            None => return queries,
        };

        let table_name = match &self.table_name {
            Some(t) => t,
            None => return queries,
        };

        // Group modifications by row
        let mut rows_to_update: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
        for ((row, col), value) in &self.modified_cells {
            rows_to_update.entry(*row).or_insert_with(Vec::new).push((*col, value.clone()));
        }

        // Generate UPDATE query for each modified row
        for (row_idx, modifications) in rows_to_update {
            if let Some(row_data) = result.rows.get(row_idx) {
                let mut set_clauses = Vec::new();
                let mut where_clauses = Vec::new();

                // Build SET clauses for modified columns
                for (col_idx, new_value) in &modifications {
                    if let Some(col_name) = result.columns.get(*col_idx) {
                        set_clauses.push(format!("{} = '{}'", col_name, new_value.replace("'", "''")));
                    }
                }

                // Build WHERE clause using all original column values
                for (col_idx, col_name) in result.columns.iter().enumerate() {
                    if let Some(original_value) = row_data.get(col_idx) {
                        where_clauses.push(format!("{} = '{}'", col_name, original_value.replace("'", "''")));
                    }
                }

                if !set_clauses.is_empty() && !where_clauses.is_empty() {
                    let query = format!(
                        "UPDATE {} SET {} WHERE {}",
                        table_name,
                        set_clauses.join(", "),
                        where_clauses.join(" AND ")
                    );
                    queries.push(query);
                }
            }
        }

        queries
    }

    pub fn clear_modifications(&mut self) {
        self.modified_cells.clear();
    }

    pub fn has_insert_data(&self) -> bool {
        !self.insert_row.is_empty()
    }

    pub fn generate_insert_query(&self) -> Option<String> {
        let result = self.result.as_ref()?;
        let table_name = self.table_name.as_ref()?;

        if self.insert_row.is_empty() {
            return None;
        }

        let mut columns = Vec::new();
        let mut values = Vec::new();

        // Build columns and values lists
        for (col_idx, value) in &self.insert_row {
            if let Some(col_name) = result.columns.get(*col_idx) {
                columns.push(col_name.clone());
                values.push(format!("'{}'", value.replace("'", "''")));
            }
        }

        if columns.is_empty() {
            return None;
        }

        Some(format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            columns.join(", "),
            values.join(", ")
        ))
    }

    pub fn clear_insert_data(&mut self) {
        self.insert_row.clear();
    }

    pub fn discard_all_changes(&mut self) {
        self.modified_cells.clear();
        self.insert_row.clear();
        self.edit_buffer.clear();
        if self.edit_mode {
            self.exit_edit_mode();
        }
        if self.insert_mode {
            self.exit_insert_mode();
        }
    }

    pub fn discard_schema_changes(&mut self) {
        self.schema_modified_cells.clear();
        self.schema_insert_row.clear();
        self.schema_edit_buffer.clear();
        if self.schema_edit_mode {
            self.exit_schema_edit_mode();
        }
        if self.schema_insert_mode {
            self.exit_schema_insert_mode();
        }
    }

    pub fn has_any_changes(&self) -> bool {
        !self.modified_cells.is_empty() || !self.insert_row.is_empty() ||
        !self.schema_modified_cells.is_empty() || !self.schema_insert_row.is_empty()
    }

    pub fn switch_to_data_tab(&mut self) {
        self.active_tab = TabMode::Data;
    }

    pub fn switch_to_schema_tab(&mut self) {
        self.active_tab = TabMode::Schema;
    }

    pub fn switch_to_indexes_tab(&mut self) {
        self.active_tab = TabMode::Indexes;
    }

    pub fn set_schema_info(&mut self, schema: String) {
        self.schema_info = Some(schema);
    }

    pub fn set_schema_columns(&mut self, columns: Vec<ColumnInfo>) {
        self.schema_columns = columns;
        self.schema_table_state.select(Some(0));
    }

    pub fn set_indexes_info(&mut self, indexes: Vec<String>) {
        self.indexes_info = Some(indexes);
    }

    pub fn enter_schema_edit_mode(&mut self) {
        if !self.schema_columns.is_empty() {
            self.schema_edit_mode = true;
            let selected_row = self.schema_table_state.selected().unwrap_or(0);
            let col_idx = self.schema_selected_column;
            // Initialize edit buffer with current value
            let current_value = self.get_schema_cell_value(selected_row, col_idx);
            self.schema_edit_buffer = current_value;
        }
    }

    pub fn exit_schema_edit_mode(&mut self) {
        self.schema_edit_mode = false;
        self.schema_edit_buffer.clear();
    }

    pub fn enter_schema_insert_mode(&mut self) {
        self.schema_insert_mode = true;
        self.schema_selected_column = 0;
        self.schema_edit_buffer.clear();
    }

    pub fn exit_schema_insert_mode(&mut self) {
        self.schema_insert_mode = false;
        self.schema_insert_row.clear();
        self.schema_edit_buffer.clear();
    }

    pub fn schema_move_column_left(&mut self) {
        if self.schema_selected_column > 0 {
            self.save_schema_cell_edit();
            self.schema_selected_column -= 1;
            let selected_row = self.schema_table_state.selected().unwrap_or(0);
            let current_value = self.get_schema_cell_value(selected_row, self.schema_selected_column);
            self.schema_edit_buffer = current_value;
        }
    }

    pub fn schema_move_column_right(&mut self) {
        if self.schema_selected_column < 4 { // 5 columns: name, type, nullable, default, extra
            self.save_schema_cell_edit();
            self.schema_selected_column += 1;
            let selected_row = self.schema_table_state.selected().unwrap_or(0);
            let current_value = self.get_schema_cell_value(selected_row, self.schema_selected_column);
            self.schema_edit_buffer = current_value;
        }
    }

    pub fn schema_insert_char(&mut self, c: char) {
        self.schema_edit_buffer.push(c);
    }

    pub fn schema_backspace(&mut self) {
        self.schema_edit_buffer.pop();
    }

    pub fn save_schema_cell_edit(&mut self) {
        let selected_row = self.schema_table_state.selected().unwrap_or(0);
        let col_idx = self.schema_selected_column;
        self.schema_modified_cells.insert((selected_row, col_idx), self.schema_edit_buffer.clone());
    }

    pub fn save_schema_insert_field(&mut self) {
        self.schema_insert_row.insert(self.schema_selected_column, self.schema_edit_buffer.clone());
        self.schema_edit_buffer.clear();
    }

    fn get_schema_cell_value(&self, row: usize, col: usize) -> String {
        // Check if there's a modified value first
        if let Some(modified) = self.schema_modified_cells.get(&(row, col)) {
            return modified.clone();
        }

        // Otherwise get from schema_columns
        if let Some(column_info) = self.schema_columns.get(row) {
            match col {
                0 => column_info.name.clone(),
                1 => column_info.data_type.clone(),
                2 => column_info.nullable.clone(),
                3 => column_info.default_value.clone(),
                4 => column_info.extra.clone(),
                _ => String::new(),
            }
        } else {
            String::new()
        }
    }

    pub fn schema_move_up(&mut self) {
        let selected = self.schema_table_state.selected().unwrap_or(0);
        if selected > 0 {
            self.schema_table_state.select(Some(selected - 1));
        }
    }

    pub fn schema_move_down(&mut self) {
        if !self.schema_columns.is_empty() {
            let selected = self.schema_table_state.selected().unwrap_or(0);
            if selected < self.schema_columns.len() - 1 {
                self.schema_table_state.select(Some(selected + 1));
            }
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        use ratatui::layout::{Direction, Layout};

        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if let Some(ref result) = self.result {
            // Split area into tabs and content
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Tabs
                    Constraint::Min(0),    // Content
                ])
                .split(area);

            // Render tabs
            let tab_titles = vec!["1. Data", "2. Schema", "3. Indexes"];
            let tabs = Tabs::new(tab_titles)
                .block(Block::default().borders(Borders::ALL).border_style(border_style))
                .select(match self.active_tab {
                    TabMode::Data => 0,
                    TabMode::Schema => 1,
                    TabMode::Indexes => 2,
                })
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

            frame.render_widget(tabs, chunks[0]);

            // Render content based on active tab
            match self.active_tab {
                TabMode::Data => self.render_data_tab(frame, chunks[1], border_style),
                TabMode::Schema => self.render_schema_tab(frame, chunks[1], border_style),
                TabMode::Indexes => self.render_indexes_tab(frame, chunks[1], border_style),
            }
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Results ")
                .border_style(border_style);
            frame.render_widget(block, area);
        }
    }

    fn render_data_tab(&mut self, frame: &mut Frame, area: Rect, border_style: Style) {
        if let Some(ref result) = self.result {
            // Calculate how many columns can fit in the available width
            // Assume each column needs at least 15 characters (to accommodate headers)
            let available_width = area.width.saturating_sub(4); // Account for borders
            self.visible_columns = ((available_width / 15).max(1) as usize).min(result.columns.len());

            // Determine which columns to display based on horizontal scroll
            let start_col = self.horizontal_scroll;
            let end_col = (start_col + self.visible_columns).min(result.columns.len());
            
            // Create header with visible columns only - make it more prominent
            let visible_columns = &result.columns[start_col..end_col];
            let header_cells = visible_columns
                .iter()
                .enumerate()
                .map(|(idx, h)| {
                    let col_idx = start_col + idx;
                    let style = if col_idx == self.selected_column {
                        // Highlight selected column header
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    };
                    Cell::from(format!(" {} ", h)).style(style)
                });
            let header = Row::new(header_cells)
                .height(1)
                .bottom_margin(0);

            let selected_row = self.table_state.selected().unwrap_or(0);

            // Create rows including insert row if in insert mode
            let mut all_rows: Vec<Row> = Vec::new();

            // Add insert row at the top if in insert mode
            if self.insert_mode {
                let insert_cells = (start_col..end_col).map(|col_idx| {
                    if col_idx == self.selected_column {
                        // Show edit buffer for currently editing cell
                        Cell::from(format!(" {} ", self.edit_buffer.clone()))
                            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else if let Some(value) = self.insert_row.get(&col_idx) {
                        // Show entered value
                        Cell::from(format!(" {} ", value))
                            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC))
                    } else {
                        // Show empty cell
                        Cell::from(" ")
                    }
                });
                let insert_row = Row::new(insert_cells)
                    .height(1)
                    .style(Style::default().bg(Color::Rgb(50, 50, 50)).add_modifier(Modifier::BOLD));
                all_rows.push(insert_row);
            }

            // Add existing data rows with horizontal scrolling
            let data_rows = result.rows.iter().enumerate().map(|(row_idx, row)| {
                let cells = (start_col..end_col).map(|col_idx| {
                    let cell_value = row.get(col_idx).map(String::as_str).unwrap_or("");
                    let is_selected_column = col_idx == self.selected_column;

                    // Check if this is the currently editing cell
                    if self.edit_mode && row_idx == selected_row && col_idx == self.selected_column {
                        // Show edit buffer for currently editing cell with padding
                        Cell::from(format!(" {} ", self.edit_buffer.clone()))
                            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else if let Some(modified_value) = self.modified_cells.get(&(row_idx, col_idx)) {
                        // Show modified value for edited cells with padding
                        let mut style = Style::default().fg(Color::Green).add_modifier(Modifier::ITALIC);
                        if is_selected_column && !self.edit_mode {
                            style = style.bg(Color::Rgb(60, 60, 80));
                        }
                        Cell::from(format!(" {} ", modified_value)).style(style)
                    } else {
                        // Show original value with padding, highlight if selected column
                        let mut style = Style::default();
                        if is_selected_column && !self.edit_mode {
                            style = style.bg(Color::Rgb(60, 60, 80));
                        }
                        Cell::from(format!(" {} ", cell_value)).style(style)
                    }
                });
                Row::new(cells).height(1)
            });

            all_rows.extend(data_rows);

            // Create column widths for visible columns - calculate based on content
            let widths = (start_col..end_col)
                .map(|col_idx| {
                    // Calculate max width needed for this column
                    let header_width = result.columns.get(col_idx).map(|h| h.len()).unwrap_or(0);
                    let max_content_width = result.rows.iter()
                        .filter_map(|row| row.get(col_idx))
                        .map(|cell| cell.len())
                        .max()
                        .unwrap_or(0);

                    // Use the larger of header or content, with min of 10
                    // Use Min constraint to allow columns to be flexible but ensure minimum width
                    let width = header_width.max(max_content_width).max(10);
                    Constraint::Min(width as u16 + 2) // +2 for padding
                })
                .collect::<Vec<_>>();

            let mut title = format!(" Results ({} rows) ", result.rows.len());
            if let Some(affected) = result.rows_affected {
                title = format!(" {} rows affected ", affected);
            }
            if result.execution_time_ms > 0 {
                title.push_str(&format!("- {}ms ", result.execution_time_ms));
            }
            
            // Add scroll information to title
            if result.columns.len() > self.visible_columns {
                title.push_str(&format!("- Cols {}-{}/{} ", 
                    start_col + 1, 
                    end_col, 
                    result.columns.len()
                ));
            }
            
            if self.insert_mode {
                title.push_str("- [INSERT MODE] ");
            }
            if self.edit_mode {
                title.push_str("- [EDIT MODE] ");
            }
            if !self.modified_cells.is_empty() {
                title.push_str(&format!("- {} changes ", self.modified_cells.len()));
            }
            if !self.insert_row.is_empty() {
                title.push_str(&format!("- {} fields ", self.insert_row.len()));
            }

            // Add help hints
            if !self.modified_cells.is_empty() || !self.insert_row.is_empty() {
                title.push_str("- Ctrl+D: Discard, Ctrl+S: Save ");
            }
            title.push_str("- R: Refresh");

            // Add status message if present
            if let Some(ref msg) = self.status_message {
                title.push_str(&format!(" | {} ", msg));
            }

            let table = Table::new(all_rows, widths)
                .header(header)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(border_style),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Rgb(70, 70, 90))
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .column_spacing(1);

            frame.render_stateful_widget(table, area, &mut self.table_state);
        }
    }

    fn render_schema_tab(&mut self, frame: &mut Frame, area: Rect, border_style: Style) {
        if self.schema_columns.is_empty() {
            let paragraph = Paragraph::new("No schema information available.\nPress Enter on a table to load schema.")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Table Schema ")
                        .border_style(border_style),
                )
                .style(Style::default().fg(Color::White));
            frame.render_widget(paragraph, area);
            return;
        }

        // Create header
        let header_cells = vec!["Column Name", "Data Type", "Nullable", "Default", "Extra"]
            .into_iter()
            .map(|h| Cell::from(format!(" {} ", h)).style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            ));
        let header = Row::new(header_cells).height(1);

        let selected_row = self.schema_table_state.selected().unwrap_or(0);

        // Create rows
        let mut all_rows: Vec<Row> = Vec::new();

        // Add insert row if in insert mode
        if self.schema_insert_mode {
            let insert_cells = (0..5).map(|col_idx| {
                if col_idx == self.schema_selected_column {
                    Cell::from(format!(" {} ", self.schema_edit_buffer.clone()))
                        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else if let Some(value) = self.schema_insert_row.get(&col_idx) {
                    Cell::from(format!(" {} ", value))
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC))
                } else {
                    Cell::from(" ")
                }
            });
            let insert_row = Row::new(insert_cells)
                .height(1)
                .style(Style::default().bg(Color::Rgb(50, 50, 50)).add_modifier(Modifier::BOLD));
            all_rows.push(insert_row);
        }

        // Add column data rows
        let data_rows = self.schema_columns.iter().enumerate().map(|(row_idx, _)| {
            let cells = (0..5).map(|col_idx| {
                let cell_value = self.get_schema_cell_value(row_idx, col_idx);

                if self.schema_edit_mode && row_idx == selected_row && col_idx == self.schema_selected_column {
                    Cell::from(format!(" {} ", self.schema_edit_buffer.clone()))
                        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else if self.schema_modified_cells.contains_key(&(row_idx, col_idx)) {
                    Cell::from(format!(" {} ", cell_value))
                        .style(Style::default().fg(Color::Green).add_modifier(Modifier::ITALIC))
                } else {
                    Cell::from(format!(" {} ", cell_value))
                }
            });
            Row::new(cells).height(1)
        });

        all_rows.extend(data_rows);

        // Column widths
        let widths = vec![
            Constraint::Min(20), // Column Name
            Constraint::Min(15), // Data Type
            Constraint::Min(10), // Nullable
            Constraint::Min(15), // Default
            Constraint::Min(20), // Extra
        ];

        let mut title = format!(" Table Schema ({} columns) ", self.schema_columns.len());
        if self.schema_insert_mode {
            title.push_str("- [INSERT MODE] ");
        }
        if self.schema_edit_mode {
            title.push_str("- [EDIT MODE] ");
        }
        if !self.schema_modified_cells.is_empty() {
            title.push_str(&format!("- {} changes ", self.schema_modified_cells.len()));
        }

        // Add help hints
        if !self.schema_modified_cells.is_empty() || !self.schema_insert_row.is_empty() {
            title.push_str("- Ctrl+D: Discard, Ctrl+S: Save, ");
        } else {
            title.push_str("- e: Edit, Ctrl+N: Add column, ");
        }
        title.push_str("R: Refresh");

        let table = Table::new(all_rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(70, 70, 90))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .column_spacing(1);

        frame.render_stateful_widget(table, area, &mut self.schema_table_state);
    }

    fn render_indexes_tab(&mut self, frame: &mut Frame, area: Rect, border_style: Style) {
        let content = if let Some(ref indexes) = self.indexes_info {
            if indexes.is_empty() {
                "No indexes found for this table.".to_string()
            } else {
                indexes.join("\n\n")
            }
        } else {
            "No index information available.\nPress Enter on a table to load indexes.".to_string()
        };

        let paragraph = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Table Indexes ")
                    .border_style(border_style),
            )
            .style(Style::default().fg(Color::White))
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}


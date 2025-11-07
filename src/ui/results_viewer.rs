use crate::db::QueryResult;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};
use std::collections::HashMap;

#[derive(Debug)]
pub struct ResultsViewer {
    pub result: Option<QueryResult>,
    pub table_state: TableState,
    pub scroll_offset: usize,
    pub focused: bool,
    pub edit_mode: bool,
    pub insert_mode: bool,
    pub selected_column: usize,
    pub modified_cells: HashMap<(usize, usize), String>, // (row, col) -> new value
    pub insert_row: HashMap<usize, String>, // col_idx -> new value for insert
    pub table_name: Option<String>,
    pub edit_buffer: String,
}

impl ResultsViewer {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));

        Self {
            result: None,
            table_state: state,
            scroll_offset: 0,
            focused: false,
            edit_mode: false,
            insert_mode: false,
            selected_column: 0,
            modified_cells: HashMap::new(),
            insert_row: HashMap::new(),
            table_name: None,
            edit_buffer: String::new(),
        }
    }

    pub fn set_result(&mut self, result: QueryResult) {
        self.result = Some(result);
        self.scroll_offset = 0;
        self.table_state.select(Some(0));
    }

    pub fn clear(&mut self) {
        self.result = None;
        self.scroll_offset = 0;
        self.table_state.select(Some(0));
    }

    pub fn move_up(&mut self, count: usize) {
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

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if let Some(ref result) = self.result {
            let header_cells = result
                .columns
                .iter()
                .map(|h| Cell::from(h.clone()).style(Style::default().fg(Color::Yellow)));
            let header = Row::new(header_cells)
                .style(Style::default().bg(Color::DarkGray))
                .height(1);

            let selected_row = self.table_state.selected().unwrap_or(0);

            // Create rows including insert row if in insert mode
            let mut all_rows: Vec<Row> = Vec::new();

            // Add insert row at the top if in insert mode
            if self.insert_mode {
                let insert_cells = result.columns.iter().enumerate().map(|(col_idx, _)| {
                    if col_idx == self.selected_column {
                        // Show edit buffer for currently editing cell
                        let mut cell = Cell::from(self.edit_buffer.clone());
                        cell = cell.style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
                        cell
                    } else if let Some(value) = self.insert_row.get(&col_idx) {
                        // Show entered value
                        let mut cell = Cell::from(value.clone());
                        cell = cell.style(Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC));
                        cell
                    } else {
                        // Show empty cell
                        Cell::from("")
                    }
                });
                let insert_row = Row::new(insert_cells)
                    .height(1)
                    .style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
                all_rows.push(insert_row);
            }

            // Add existing data rows
            let data_rows = result.rows.iter().enumerate().map(|(row_idx, row)| {
                let cells = row.iter().enumerate().map(|(col_idx, c)| {
                    // Check if this is the currently editing cell
                    if self.edit_mode && row_idx == selected_row && col_idx == self.selected_column {
                        // Show edit buffer for currently editing cell
                        let mut cell = Cell::from(self.edit_buffer.clone());
                        // Highlight the editing cell
                        cell = cell.style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
                        cell
                    } else if let Some(modified_value) = self.modified_cells.get(&(row_idx, col_idx)) {
                        // Show modified value for edited cells
                        let mut cell = Cell::from(modified_value.clone());
                        // Mark modified cells with a different color
                        cell = cell.style(Style::default().fg(Color::Green).add_modifier(Modifier::ITALIC));
                        cell
                    } else {
                        // Show original value
                        Cell::from(c.clone())
                    }
                });
                Row::new(cells).height(1)
            });

            all_rows.extend(data_rows);

            let widths = result
                .columns
                .iter()
                .map(|_| Constraint::Percentage((100 / result.columns.len().max(1)) as u16))
                .collect::<Vec<_>>();

            let mut title = format!(" Results ({} rows) ", result.rows.len());
            if let Some(affected) = result.rows_affected {
                title = format!(" {} rows affected ", affected);
            }
            if result.execution_time_ms > 0 {
                title.push_str(&format!("- {}ms ", result.execution_time_ms));
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
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );

            frame.render_stateful_widget(table, area, &mut self.table_state);
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Results ")
                .border_style(border_style);
            frame.render_widget(block, area);
        }
    }
}

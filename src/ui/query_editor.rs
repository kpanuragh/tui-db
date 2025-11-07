use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct QueryEditor {
    pub content: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub focused: bool,
    pub visual_start: Option<(usize, usize)>,
    pub scroll_offset: usize,
}

impl QueryEditor {
    pub fn new() -> Self {
        Self {
            content: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            focused: false,
            visual_start: None,
            scroll_offset: 0,
        }
    }

    pub fn get_query(&self) -> String {
        self.content.join("\n")
    }

    pub fn get_query_at_cursor(&self) -> String {
        let full_text = self.content.join("\n");

        // Calculate cursor position in the full text
        let mut cursor_pos = 0;
        for i in 0..self.cursor_line {
            cursor_pos += self.content[i].len() + 1; // +1 for newline
        }
        cursor_pos += self.cursor_col;

        // Find the start of the query (previous semicolon or start of text)
        let start = full_text[..cursor_pos]
            .rfind(';')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        // Find the end of the query (next semicolon or end of text)
        let end = full_text[cursor_pos..]
            .find(';')
            .map(|pos| cursor_pos + pos + 1)
            .unwrap_or(full_text.len());

        // Extract and trim the query
        full_text[start..end].trim().to_string()
    }

    pub fn clear(&mut self) {
        self.content = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll_offset = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        if self.cursor_line >= self.content.len() {
            self.content.push(String::new());
        }
        self.content[self.cursor_line].insert(self.cursor_col, c);
        self.cursor_col += 1;
    }

    pub fn insert_newline(&mut self) {
        if self.cursor_line >= self.content.len() {
            self.content.push(String::new());
        }

        let current_line = &self.content[self.cursor_line];
        let after = current_line[self.cursor_col..].to_string();
        self.content[self.cursor_line].truncate(self.cursor_col);
        self.content.insert(self.cursor_line + 1, after);
        self.cursor_line += 1;
        self.cursor_col = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.content[self.cursor_line].remove(self.cursor_col - 1);
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            let current_line = self.content.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.content[self.cursor_line].len();
            self.content[self.cursor_line].push_str(&current_line);
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor_col < self.content[self.cursor_line].len() {
            self.content[self.cursor_line].remove(self.cursor_col);
        } else if self.cursor_line < self.content.len() - 1 {
            let next_line = self.content.remove(self.cursor_line + 1);
            self.content[self.cursor_line].push_str(&next_line);
        }
    }

    pub fn move_left(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_col > 0 {
                self.cursor_col -= 1;
            } else if self.cursor_line > 0 {
                self.cursor_line -= 1;
                self.cursor_col = self.content[self.cursor_line].len();
            }
        }
    }

    pub fn move_right(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_col < self.content[self.cursor_line].len() {
                self.cursor_col += 1;
            } else if self.cursor_line < self.content.len() - 1 {
                self.cursor_line += 1;
                self.cursor_col = 0;
            }
        }
    }

    pub fn move_up(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_line > 0 {
                self.cursor_line -= 1;
                self.cursor_col = self.cursor_col.min(self.content[self.cursor_line].len());
            }
        }
    }

    pub fn move_down(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_line < self.content.len() - 1 {
                self.cursor_line += 1;
                self.cursor_col = self.cursor_col.min(self.content[self.cursor_line].len());
            }
        }
    }

    pub fn goto_line_start(&mut self) {
        self.cursor_col = 0;
    }

    pub fn goto_line_end(&mut self) {
        self.cursor_col = self.content[self.cursor_line].len();
    }

    pub fn goto_top(&mut self) {
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll_offset = 0;
    }

    pub fn goto_bottom(&mut self) {
        self.cursor_line = self.content.len() - 1;
        self.cursor_col = self.content[self.cursor_line].len();
    }

    pub fn start_visual_mode(&mut self) {
        self.visual_start = Some((self.cursor_line, self.cursor_col));
    }

    pub fn exit_visual_mode(&mut self) {
        self.visual_start = None;
    }

    pub fn get_selection(&self) -> Option<String> {
        if let Some((start_line, start_col)) = self.visual_start {
            let (end_line, end_col) = (self.cursor_line, self.cursor_col);

            let (start, end) = if (start_line, start_col) <= (end_line, end_col) {
                ((start_line, start_col), (end_line, end_col))
            } else {
                ((end_line, end_col), (start_line, start_col))
            };

            if start.0 == end.0 {
                Some(self.content[start.0][start.1..end.1].to_string())
            } else {
                let mut result = String::new();
                result.push_str(&self.content[start.0][start.1..]);
                for line in (start.0 + 1)..end.0 {
                    result.push('\n');
                    result.push_str(&self.content[line]);
                }
                result.push('\n');
                result.push_str(&self.content[end.0][..end.1]);
                Some(result)
            }
        } else {
            None
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let text: Vec<Line> = self
            .content
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let line_num = format!("{:3} ", i + 1);
                if i == self.cursor_line && self.focused {
                    Line::from(vec![
                        Span::styled(line_num, Style::default().fg(Color::DarkGray)),
                        Span::styled(line.clone(), Style::default().fg(Color::White)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(line_num, Style::default().fg(Color::DarkGray)),
                        Span::raw(line.clone()),
                    ])
                }
            })
            .collect();

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Query Editor ")
                    .border_style(border_style),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);

        // Render cursor if focused
        if self.focused {
            let cursor_x = area.x + 5 + self.cursor_col as u16;
            let cursor_y = area.y + 1 + (self.cursor_line - self.scroll_offset) as u16;
            if cursor_y < area.y + area.height - 1 {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }
}

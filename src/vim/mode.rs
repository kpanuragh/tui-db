use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
    Command,
}

impl VimMode {
    pub fn as_str(&self) -> &str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
            VimMode::Command => "COMMAND",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VimState {
    pub mode: VimMode,
    pub command_buffer: String,
    pub register: Option<String>,
    pub count: Option<usize>,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            mode: VimMode::Normal,
            command_buffer: String::new(),
            register: None,
            count: None,
        }
    }
}

impl VimState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter_insert_mode(&mut self) {
        self.mode = VimMode::Insert;
        self.reset_state();
    }

    pub fn enter_normal_mode(&mut self) {
        self.mode = VimMode::Normal;
        self.reset_state();
    }

    pub fn enter_visual_mode(&mut self) {
        self.mode = VimMode::Visual;
        self.reset_state();
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = VimMode::Command;
        self.command_buffer.clear();
    }

    pub fn append_to_command(&mut self, ch: char) {
        if self.mode == VimMode::Command {
            self.command_buffer.push(ch);
        }
    }

    pub fn backspace_command(&mut self) {
        if self.mode == VimMode::Command {
            self.command_buffer.pop();
        }
    }

    pub fn get_command(&self) -> &str {
        &self.command_buffer
    }

    pub fn clear_command(&mut self) {
        self.command_buffer.clear();
    }

    pub fn set_count(&mut self, n: usize) {
        self.count = Some(match self.count {
            Some(existing) => existing * 10 + n,
            None => n,
        });
    }

    pub fn get_count(&self) -> usize {
        self.count.unwrap_or(1)
    }

    fn reset_state(&mut self) {
        self.command_buffer.clear();
        self.count = None;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match self.mode {
            VimMode::Normal => self.handle_normal_mode(key),
            VimMode::Insert => self.handle_insert_mode(key),
            VimMode::Visual => self.handle_visual_mode(key),
            VimMode::Command => self.handle_command_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('c') => Some(VimCommand::Quit),
                KeyCode::Char('e') => Some(VimCommand::ExecuteQueryUnderCursor),
                KeyCode::Char('r') => Some(VimCommand::ExecuteAllQueries),
                KeyCode::Char('s') => Some(VimCommand::SaveAllEdits),
                KeyCode::Char('n') => Some(VimCommand::EnterInsertRowMode),
                _ => None,
            };
        }

        match key.code {
            KeyCode::Char('h') | KeyCode::Left => Some(VimCommand::MoveLeft(self.get_count())),
            KeyCode::Char('j') | KeyCode::Down => Some(VimCommand::MoveDown(self.get_count())),
            KeyCode::Char('k') | KeyCode::Up => Some(VimCommand::MoveUp(self.get_count())),
            KeyCode::Char('l') | KeyCode::Right => Some(VimCommand::MoveRight(self.get_count())),
            KeyCode::Char('e') => Some(VimCommand::EnterEditMode),
            KeyCode::Char('i') => {
                self.enter_insert_mode();
                Some(VimCommand::EnterInsertMode)
            }
            KeyCode::Char('a') => {
                self.enter_insert_mode();
                Some(VimCommand::EnterInsertModeAfter)
            }
            KeyCode::Char('I') => {
                self.enter_insert_mode();
                Some(VimCommand::InsertAtLineStart)
            }
            KeyCode::Char('A') => {
                self.enter_insert_mode();
                Some(VimCommand::InsertAtLineEnd)
            }
            KeyCode::Char('o') => {
                self.enter_insert_mode();
                Some(VimCommand::OpenLineBelow)
            }
            KeyCode::Char('O') => {
                self.enter_insert_mode();
                Some(VimCommand::OpenLineAbove)
            }
            KeyCode::Char('v') => {
                self.enter_visual_mode();
                Some(VimCommand::EnterVisualMode)
            }
            KeyCode::Char(':') => {
                self.enter_command_mode();
                Some(VimCommand::EnterCommandMode)
            }
            KeyCode::Char('x') => Some(VimCommand::DeleteChar),
            KeyCode::Char('X') => Some(VimCommand::DeleteConnection),
            KeyCode::Char('C') => Some(VimCommand::OpenConnectionManager),
            KeyCode::Char('d') => Some(VimCommand::Delete),
            KeyCode::Char('y') => Some(VimCommand::Yank),
            KeyCode::Char('p') => Some(VimCommand::Paste),
            KeyCode::Char('u') => Some(VimCommand::Undo),
            KeyCode::Char('r') => Some(VimCommand::Redo),
            KeyCode::Char('R') => Some(VimCommand::RefreshData),
            KeyCode::Char('g') if matches!(self.command_buffer.chars().last(), Some('g')) => {
                self.command_buffer.clear();
                Some(VimCommand::GotoTop)
            }
            KeyCode::Char('g') => {
                self.command_buffer.push('g');
                None
            }
            KeyCode::Char('G') => Some(VimCommand::GotoBottom),
            KeyCode::Char('0') if self.count.is_none() => Some(VimCommand::GotoLineStart),
            KeyCode::Char('$') => Some(VimCommand::GotoLineEnd),
            KeyCode::Char('w') => Some(VimCommand::NextWord),
            KeyCode::Char('b') => Some(VimCommand::PrevWord),
            KeyCode::Char('/') => {
                self.enter_command_mode();
                self.command_buffer.push('/');
                Some(VimCommand::StartSearch)
            }
            KeyCode::Char('n') => Some(VimCommand::NextMatch),
            KeyCode::Char('N') => Some(VimCommand::PrevMatch),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let digit = c.to_digit(10).unwrap() as usize;
                self.set_count(digit);
                None
            }
            KeyCode::Tab => Some(VimCommand::NextPane),
            KeyCode::BackTab => Some(VimCommand::PrevPane),
            KeyCode::Enter => Some(VimCommand::Activate),
            _ => None,
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match key.code {
            KeyCode::Esc => {
                self.enter_normal_mode();
                Some(VimCommand::ExitInsertMode)
            }
            KeyCode::Char(c) => Some(VimCommand::InsertChar(c)),
            KeyCode::Backspace => Some(VimCommand::Backspace),
            KeyCode::Enter => Some(VimCommand::InsertNewline),
            KeyCode::Tab => Some(VimCommand::InsertTab),
            KeyCode::Left => Some(VimCommand::MoveLeft(1)),
            KeyCode::Right => Some(VimCommand::MoveRight(1)),
            KeyCode::Up => Some(VimCommand::MoveUp(1)),
            KeyCode::Down => Some(VimCommand::MoveDown(1)),
            _ => None,
        }
    }

    fn handle_visual_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match key.code {
            KeyCode::Esc => {
                self.enter_normal_mode();
                Some(VimCommand::ExitVisualMode)
            }
            KeyCode::Char('h') | KeyCode::Left => Some(VimCommand::ExtendLeft),
            KeyCode::Char('j') | KeyCode::Down => Some(VimCommand::ExtendDown),
            KeyCode::Char('k') | KeyCode::Up => Some(VimCommand::ExtendUp),
            KeyCode::Char('l') | KeyCode::Right => Some(VimCommand::ExtendRight),
            KeyCode::Char('y') => {
                self.enter_normal_mode();
                Some(VimCommand::YankSelection)
            }
            KeyCode::Char('d') => {
                self.enter_normal_mode();
                Some(VimCommand::DeleteSelection)
            }
            _ => None,
        }
    }

    fn handle_command_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match key.code {
            KeyCode::Esc => {
                self.enter_normal_mode();
                Some(VimCommand::CancelCommand)
            }
            KeyCode::Enter => {
                let cmd = self.command_buffer.clone();
                self.enter_normal_mode();
                Some(VimCommand::ExecuteCommand(cmd))
            }
            KeyCode::Char(c) => {
                self.append_to_command(c);
                None
            }
            KeyCode::Backspace => {
                self.backspace_command();
                if self.command_buffer.is_empty() {
                    self.enter_normal_mode();
                    return Some(VimCommand::CancelCommand);
                }
                None
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VimCommand {
    // Movement
    MoveLeft(usize),
    MoveRight(usize),
    MoveUp(usize),
    MoveDown(usize),
    GotoTop,
    GotoBottom,
    GotoLineStart,
    GotoLineEnd,
    NextWord,
    PrevWord,

    // Mode changes
    EnterInsertMode,
    EnterInsertModeAfter,
    InsertAtLineStart,
    InsertAtLineEnd,
    OpenLineBelow,
    OpenLineAbove,
    ExitInsertMode,
    EnterVisualMode,
    ExitVisualMode,
    EnterCommandMode,
    CancelCommand,

    // Editing
    InsertChar(char),
    InsertNewline,
    InsertTab,
    Backspace,
    DeleteChar,
    Delete,
    DeleteSelection,
    Yank,
    YankSelection,
    Paste,
    Undo,
    Redo,

    // Visual mode
    ExtendLeft,
    ExtendRight,
    ExtendUp,
    ExtendDown,

    // Search
    StartSearch,
    NextMatch,
    PrevMatch,

    // Navigation
    NextPane,
    PrevPane,

    // Actions
    Activate,
    ExecuteQueryUnderCursor,
    ExecuteAllQueries,

    // Edit Mode
    EnterEditMode,
    ExitEditMode,
    MoveColumnLeft,
    MoveColumnRight,
    SaveCellEdit,
    SaveAllEdits,

    // Insert Mode
    EnterInsertRowMode,
    ExitInsertRowMode,
    SaveInsertField,
    SaveInsertRow,

    // Connection Management
    DeleteConnection,
    OpenConnectionManager,
    CloseConnectionManager,
    ConnectionManagerAction(char),  // n, e, d, t, Enter

    // Results Viewer Tabs
    SwitchToDataTab,
    SwitchToSchemaTab,
    SwitchToIndexesTab,

    // Data Management
    DiscardChanges,
    RefreshData,

    // Commands
    ExecuteCommand(String),
    Quit,
}

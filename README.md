# TUI-DB

A Terminal User Interface (TUI) database manager similar to DBeaver, with full Vim-like key bindings.

## Features

- **Full Vim Modal Editing**: Navigate and edit with Normal, Insert, Visual, and Command modes
- **Multi-Database Support**:
  - **SQLite**: Browse and query SQLite databases
  - **MySQL**: Connect to MySQL databases
  - **MariaDB**: Connect to MariaDB databases
- **Database Browser**: Tree view of databases, tables, and schemas
- **SQL Query Editor**: Write and execute SQL queries with syntax highlighting
- **Results Viewer**: View query results in a table format with navigation
- **Multiple Connections**: Open and switch between multiple database connections simultaneously

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/tui-db`

## Usage

```bash
# Run the application
cargo run

# Or use the binary directly
./target/release/tui-db
```

## Vim Key Bindings

### Normal Mode (Default)

#### Navigation
- `h`, `j`, `k`, `l` or `Arrow Keys` - Move left, down, up, right
- `gg` - Go to top
- `G` - Go to bottom
- `0` - Go to line start
- `$` - Go to line end
- `w` - Next word
- `b` - Previous word
- `Tab` - Next pane
- `Shift+Tab` - Previous pane
- `Enter` - Open selected table (in Database Browser)

#### Mode Switching
- `i` - Enter insert mode
- `a` - Enter insert mode after cursor
- `I` - Insert at line start
- `A` - Insert at line end
- `o` - Open new line below and enter insert mode
- `O` - Open new line above and enter insert mode
- `v` - Enter visual mode
- `:` - Enter command mode

#### Editing
- `x` - Delete character
- `d` - Delete
- `y` - Yank (copy)
- `p` - Paste
- `u` - Undo
- `r` - Redo

#### Search
- `/` - Start search
- `n` - Next match
- `N` - Previous match

#### Query Execution
- `Ctrl+E` - Execute query under cursor (in Query Editor)
- `Ctrl+R` - Execute all queries in editor (in Query Editor, displays last query result)

#### Table Data Operations
- `e` - Enter edit mode (in Results Viewer, when viewing table data)
- `Ctrl+N` - Enter insert mode to add new row (in Results Viewer)
- `h`, `l` or `Arrow Keys` - Move between columns (in edit/insert mode)
- Type to edit cell value or enter new data
- `Esc` - Save current cell and exit edit mode
- `Ctrl+S` - Save all changes to database:
  - In edit mode: generates and executes UPDATE queries
  - In insert mode: generates and executes INSERT query

### Insert Mode

- `Esc` - Return to normal mode
- Type normally to insert text
- `Backspace` - Delete character
- `Enter` - New line
- `Tab` - Insert tab
- `Arrow Keys` - Move cursor while in insert mode

### Visual Mode

- `h`, `j`, `k`, `l` or `Arrow Keys` - Extend selection
- `y` - Yank (copy) selection and return to normal mode
- `d` - Delete selection and return to normal mode
- `Esc` - Return to normal mode

### Command Mode

Available commands:
- `:q` or `:quit` - Quit application
- `:open <path>` - Open a SQLite database (file path)
- `:mysql <connection_string>` - Connect to MySQL database
- `:mariadb <connection_string>` - Connect to MariaDB database
- `:exec` or `:execute` - Execute the query in the editor
- `:clear` - Clear query editor and results
- `:disconnect` or `:close` - Close/remove the selected database connection
- `Esc` - Cancel command

#### Connection String Format

**MySQL/MariaDB:**
```
mysql://user:password@host:port/database
```

**Examples:**
```
:mysql mysql://root:password@localhost:3306/mydb
:mariadb mysql://user:pass@192.168.1.100:3306/testdb
```

### Connection Management

TUI-DB provides a powerful connection manager UI for managing database connections:

#### Opening the Connection Manager
- **`C`** (Shift+c) - Opens the Connection Manager popup in Normal mode

#### Connection Manager - List Mode

When you open the connection manager, you'll see a list of all saved connections with their database types:

- **`j`/`k` or Arrow Keys** - Navigate through connections
- **`n`** - Create a new connection (opens form)
- **`e`** - Edit the selected connection (opens form)
- **`d`** - Delete the selected connection
- **`t`** - Test the selected connection
- **`Enter`** - Connect to the selected database
- **`Esc`** - Close the connection manager

#### Connection Manager - Form Mode

When adding or editing a connection, you'll see a form with three fields:

**Field Navigation:**
- **`Tab`** - Move to next field
- **`Shift+Tab`** - Move to previous field
- **Type normally** - Enter text in Name and Connection String fields
- **`Space`** - Cycle database type (SQLite → MySQL → MariaDB) when on Type field
- **`Backspace`** - Delete characters

**Form Actions:**
- **`Ctrl+T`** - Test the connection (shows success or error message)
- **`Ctrl+S`** - Save the connection to config
- **`Esc`** - Cancel and return to list

**Form Fields:**
- **Name**: A friendly name for your connection
- **Database Type**: SQLite, MySQL, or MariaDB (cycle with Space)
- **Connection String**:
  - For SQLite: File path (e.g., `/path/to/database.db`)
  - For MySQL/MariaDB: Connection string (e.g., `mysql://user:pass@host:3306/db`)

#### Alternative Connection Methods

You can also manage connections via commands and keybindings:
- Connections are automatically saved to `~/.config/tui-db/config.json`
- Saved connections are automatically loaded on startup
- **`X`** (Shift+x) - Delete/close selected connection (when in Database Browser)
- `:disconnect` or `:close` - Remove currently selected connection
- Deleted connections are removed from saved config immediately

## Workflow

1. **Open a database**:
   - **Using Connection Manager (Recommended)**:
     - Press `C` to open the Connection Manager
     - Press `n` to add a new connection or select an existing one
     - Fill in Name, Type (cycle with Space), and Connection String
     - Press `Ctrl+T` to test (optional), then `Ctrl+S` to save
     - Press `Esc` to return to list, then `Enter` to connect
   - **Using Commands**:
     - **SQLite**: Press `:` then type `open path/to/database.db`
     - **MySQL**: Press `:` then type `mysql mysql://user:password@host:port/database`
     - **MariaDB**: Press `:` then type `mariadb mysql://user:password@host:port/database`
2. **Browse tables**: Use `j`/`k` to navigate the database browser in the left pane
3. **View table data**: Press `Enter` on a selected table to load its contents (shows first 1000 rows)
4. **Edit or insert table data** (optional):
   - **To edit existing data:**
     - Press `e` to enter edit mode
     - Use `h`/`l` or arrow keys to move between columns
     - Type to modify cell values
     - Press `Esc` to save current cell
     - Press `Ctrl+S` to save all changes to database
   - **To insert new data:**
     - Press `Ctrl+N` to enter insert mode
     - A new empty row appears at the top
     - Use `h`/`l` or arrow keys to move between columns
     - Type values for each field
     - Press `Esc` to save current field
     - Press `Ctrl+S` to insert the row into database
5. **Write a query**: Press `Tab` to switch to the query editor, then press `i` to enter insert mode
6. **Execute query**:
   - Quick execute: Press `Esc` then `Ctrl+E` to execute the query at cursor
   - Execute all: Press `Esc` then `Ctrl+R` to execute all queries in the editor
   - Or use command mode: Press `Esc` then `:exec`
7. **View results**: Results appear in the bottom pane. Press `Tab` to focus and navigate with `j`/`k`
8. **Quit**: Press `:q` in command mode, or `Ctrl+C` in normal mode

## Panes

The application is divided into three panes:

1. **Database Browser** (left) - Shows connected databases and their tables
2. **Query Editor** (top right) - Write and edit SQL queries
3. **Results Viewer** (bottom right) - View query results

Use `Tab` and `Shift+Tab` to cycle between panes.

## Configuration

Connections are saved to `~/.config/tui-db/config.json` and will be automatically loaded on startup.

## Future Enhancements

- PostgreSQL support
- Export results to CSV/JSON
- Enhanced syntax highlighting in query editor
- Auto-completion for SQL keywords and table/column names
- Query history navigation
- Multiple query tabs
- Transaction management
- Schema designer/visualizer

## Architecture

The application is built with:
- **ratatui**: TUI framework
- **crossterm**: Terminal manipulation
- **rusqlite**: SQLite database access
- **serde**: Configuration serialization

The code is organized into modules:
- `app.rs` - Main application state and event handling
- `vim/` - Vim mode system
- `db/` - Database abstraction layer
- `ui/` - UI components
- `config.rs` - Configuration management

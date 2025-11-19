use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseType {
    SQLite,
    MySQL,
    MariaDB,
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub id: usize,
    pub name: String,
    pub db_type: DatabaseType,
    pub connection_string: String, // Can be path for SQLite or connection string for MySQL
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub row_count: Option<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub rows_affected: Option<usize>,
    pub execution_time_ms: u64,
}

impl QueryResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        }
    }

    pub fn with_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = time_ms;
        self
    }

    pub fn with_affected(mut self, affected: usize) -> Self {
        self.rows_affected = Some(affected);
        self
    }
}

pub trait DatabaseConnection: Send {
    fn connect(path: &str) -> Result<Box<Self>> where Self: Sized;
    fn execute_query(&mut self, query: &str) -> Result<QueryResult>;
    fn list_tables(&mut self) -> Result<Vec<TableInfo>>;
    #[allow(dead_code)]
    fn get_table_columns(&mut self, table_name: &str) -> Result<Vec<ColumnInfo>>;
    fn get_table_data(&mut self, table_name: &str, limit: usize, offset: usize) -> Result<QueryResult>;
    #[allow(dead_code)]
    fn close(&mut self) -> Result<()>;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

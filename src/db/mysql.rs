use anyhow::Result;
use mysql::prelude::*;
use mysql::*;
use std::time::Instant;

use super::connection::{DatabaseConnection, QueryResult, TableInfo};

pub struct MySQLConnection {
    conn: PooledConn,
}

impl MySQLConnection {
    pub fn connect(connection_string: &str) -> Result<Box<dyn DatabaseConnection>> {
        let opts = Opts::from_url(connection_string)?;
        let pool = Pool::new(opts)?;
        let conn = pool.get_conn()?;

        Ok(Box::new(MySQLConnection { conn }))
    }
}

impl DatabaseConnection for MySQLConnection {
    fn connect(path: &str) -> Result<Box<Self>> where Self: Sized {
        let opts = Opts::from_url(path)?;
        let pool = Pool::new(opts)?;
        let conn = pool.get_conn()?;

        Ok(Box::new(MySQLConnection { conn }))
    }

    fn execute_query(&mut self, query: &str) -> Result<QueryResult> {
        let start = Instant::now();

        // Split query into individual statements
        let statements: Vec<&str> = query
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut last_result = QueryResult::new(Vec::new(), Vec::new());

        for stmt_text in statements {
            let stmt_upper = stmt_text.to_uppercase();

            if stmt_upper.starts_with("SELECT") || stmt_upper.starts_with("SHOW") ||
               stmt_upper.starts_with("DESCRIBE") || stmt_upper.starts_with("EXPLAIN") {
                // Query that returns rows
                let result: Vec<Row> = self.conn.query(stmt_text)?;

                if result.is_empty() {
                    last_result = QueryResult::new(Vec::new(), Vec::new());
                } else {
                    // Get column names
                    let columns: Vec<String> = result[0]
                        .columns_ref()
                        .iter()
                        .map(|col| col.name_str().to_string())
                        .collect();

                    // Convert rows to Vec<Vec<String>>
                    let mut rows = Vec::new();
                    for row in result {
                        let mut row_data = Vec::new();
                        for (idx, _col) in row.columns_ref().iter().enumerate() {
                            let value: Option<String> = row.get(idx);
                            row_data.push(value.unwrap_or_else(|| "NULL".to_string()));
                        }
                        rows.push(row_data);
                    }

                    last_result = QueryResult::new(columns, rows);
                }
            } else {
                // INSERT, UPDATE, DELETE, CREATE, DROP, etc.
                self.conn.query_drop(stmt_text)?;
                let affected = self.conn.affected_rows();
                last_result = QueryResult::new(Vec::new(), Vec::new())
                    .with_affected(affected as usize);
            }
        }

        let elapsed = start.elapsed();
        Ok(last_result.with_time(elapsed.as_millis() as u64))
    }

    fn list_tables(&mut self) -> Result<Vec<TableInfo>> {
        let query = "SHOW TABLES";
        let result: Vec<Row> = self.conn.query(query)?;

        let mut tables = Vec::new();
        for row in result {
            // Get the first column value (table name)
            let table_name: String = row.get(0).unwrap_or_default();

            // Get row count for each table
            let count_query = format!("SELECT COUNT(*) FROM `{}`", table_name);
            let count: Option<u64> = self.conn.query_first(&count_query)?;

            tables.push(TableInfo {
                name: table_name,
                row_count: count.map(|c| c as usize),
            });
        }

        Ok(tables)
    }

    fn get_table_columns(&mut self, _table_name: &str) -> Result<Vec<super::connection::ColumnInfo>> {
        // Not implemented yet
        Ok(Vec::new())
    }

    fn get_table_data(&mut self, table_name: &str, limit: usize, offset: usize) -> Result<QueryResult> {
        let query = format!("SELECT * FROM `{}` LIMIT {} OFFSET {}", table_name, limit, offset);
        self.execute_query(&query)
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

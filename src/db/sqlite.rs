use anyhow::{Context, Result};
use rusqlite::{Connection, Row};
use std::time::Instant;

use super::connection::{ColumnInfo, DatabaseConnection, QueryResult, TableInfo};

pub struct SQLiteConnection {
    conn: Connection,
}

impl SQLiteConnection {
    fn row_to_strings(row: &Row, column_count: usize) -> Result<Vec<String>> {
        let mut values = Vec::new();
        for i in 0..column_count {
            let value: Result<String, rusqlite::Error> = row.get(i);
            match value {
                Ok(v) => values.push(v),
                Err(_) => {
                    // Try other types
                    if let Ok(v) = row.get::<_, i64>(i) {
                        values.push(v.to_string());
                    } else if let Ok(v) = row.get::<_, f64>(i) {
                        values.push(v.to_string());
                    } else if let Ok(v) = row.get::<_, Vec<u8>>(i) {
                        values.push(format!("<BLOB: {} bytes>", v.len()));
                    } else {
                        values.push("NULL".to_string());
                    }
                }
            }
        }
        Ok(values)
    }
}

impl DatabaseConnection for SQLiteConnection {
    fn connect(path: &str) -> Result<Box<Self>> {
        let conn = Connection::open(path)
            .context(format!("Failed to open SQLite database at {}", path))?;
        Ok(Box::new(SQLiteConnection { conn }))
    }

    fn execute_query(&mut self, query: &str) -> Result<QueryResult> {
        let start = Instant::now();

        // Split query into individual statements
        let statements: Vec<&str> = query
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if statements.is_empty() {
            return Ok(QueryResult::new(vec![], vec![]).with_time(0));
        }

        let mut last_result = QueryResult::new(vec![], vec![]);
        let mut total_affected = 0;

        // Execute each statement
        for stmt_text in statements {
            let trimmed = stmt_text.trim().to_uppercase();

            if trimmed.starts_with("SELECT") || trimmed.starts_with("PRAGMA") {
                let mut stmt = self.conn.prepare(stmt_text)?;
                let column_names: Vec<String> = stmt
                    .column_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                let column_count = column_names.len();

                let rows = stmt
                    .query_map([], |row| Ok(Self::row_to_strings(row, column_count)))?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;

                last_result = QueryResult::new(column_names, rows);
            } else {
                let affected = self.conn.execute(stmt_text, [])?;
                total_affected += affected;
                last_result = QueryResult::new(vec![], vec![]).with_affected(affected);
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;

        // If we accumulated affected rows from multiple non-SELECT statements, update the result
        if total_affected > 0 && last_result.rows_affected.is_some() {
            last_result = last_result.with_affected(total_affected);
        }

        Ok(last_result.with_time(elapsed))
    }

    fn list_tables(&mut self) -> Result<Vec<TableInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )?;

        let tables = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                Ok(name)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut table_infos = Vec::new();
        for table in tables {
            let count: Result<usize, _> = self.conn.query_row(
                &format!("SELECT COUNT(*) FROM {}", table),
                [],
                |row| row.get(0),
            );
            table_infos.push(TableInfo {
                name: table,
                row_count: count.ok(),
            });
        }

        Ok(table_infos)
    }

    fn get_table_columns(&mut self, table_name: &str) -> Result<Vec<ColumnInfo>> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({})", table_name))?;

        let columns = stmt
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get(1)?,
                    data_type: row.get(2)?,
                    nullable: row.get::<_, i32>(3)? == 0,
                    primary_key: row.get::<_, i32>(5)? > 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(columns)
    }

    fn get_table_data(&mut self, table_name: &str, limit: usize, offset: usize) -> Result<QueryResult> {
        let query = format!(
            "SELECT * FROM {} LIMIT {} OFFSET {}",
            table_name, limit, offset
        );
        self.execute_query(&query)
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

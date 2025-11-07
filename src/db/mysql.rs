use anyhow::Result;
use mysql::prelude::*;
use mysql::*;
use std::time::Instant;

use super::connection::{DatabaseConnection, QueryResult, TableInfo};

pub struct MySQLConnection {
    conn: PooledConn,
    current_database: Option<String>,
}

impl MySQLConnection {
    pub fn connect(connection_string: &str) -> Result<Box<dyn DatabaseConnection>> {
        let opts = Opts::from_url(connection_string)?;
        let pool = Pool::new(opts)?;
        let mut conn = pool.get_conn()?;

        // Clear any database context to start with database list
        conn.query_drop("USE information_schema")?;

        Ok(Box::new(MySQLConnection { 
            conn,
            current_database: None,
        }))
    }

    pub fn use_database(&mut self, database_name: &str) -> Result<()> {
        let query = format!("USE `{}`", database_name);
        self.conn.query_drop(&query)?;
        self.current_database = Some(database_name.to_string());
        
        // Verify the database was set correctly
        let verification_result: Vec<Row> = self.conn.query("SELECT DATABASE()")?;
        if let Some(row) = verification_result.first() {
            let verified_db: Option<String> = row.get::<Option<String>, _>(0).unwrap_or(None);
            if verified_db.is_none() || verified_db.as_ref().unwrap() != database_name {
                return Err(anyhow::anyhow!("Failed to switch to database: {}", database_name));
            }
        }
        
        Ok(())
    }

    pub fn get_current_database(&mut self) -> Result<Option<String>> {
        let current_db_result: Vec<Row> = self.conn.query("SELECT DATABASE()")?;
        let current_db: Option<String> = if let Some(row) = current_db_result.first() {
            row.get::<Option<String>, _>(0).unwrap_or(None)
        } else {
            None
        };
        Ok(current_db)
    }

    pub fn clear_database_context(&mut self) -> Result<()> {
        // Switch to no database to force showing database list
        self.conn.query_drop("USE information_schema")?;
        self.current_database = None;
        Ok(())
    }
}

impl DatabaseConnection for MySQLConnection {
    fn connect(path: &str) -> Result<Box<Self>> where Self: Sized {
        let opts = Opts::from_url(path)?;
        let pool = Pool::new(opts)?;
        let mut conn = pool.get_conn()?;

        // Clear any database context to start with database list
        conn.query_drop("USE information_schema")?;

        Ok(Box::new(MySQLConnection { 
            conn,
            current_database: None,
        }))
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
                            // Try to get the value safely, handling NULLs
                            let value = match row.get_opt::<mysql::Value, _>(idx) {
                                Some(Ok(mysql::Value::NULL)) => "NULL".to_string(),
                                Some(Ok(val)) => {
                                    // Convert MySQL value to string based on its type
                                    match val {
                                        mysql::Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                                        mysql::Value::Int(i) => i.to_string(),
                                        mysql::Value::UInt(u) => u.to_string(),
                                        mysql::Value::Float(f) => f.to_string(),
                                        mysql::Value::Double(d) => d.to_string(),
                                        mysql::Value::Date(year, month, day, hour, min, sec, micro) => {
                                            if hour == 0 && min == 0 && sec == 0 && micro == 0 {
                                                format!("{:04}-{:02}-{:02}", year, month, day)
                                            } else {
                                                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, min, sec)
                                            }
                                        }
                                        mysql::Value::Time(negative, days, hours, minutes, seconds, microseconds) => {
                                            let sign = if negative { "-" } else { "" };
                                            if days > 0 {
                                                format!("{}{:02}:{:02}:{:02}", sign, days * 24 + hours as u32, minutes, seconds)
                                            } else {
                                                format!("{}{:02}:{:02}:{:02}", sign, hours, minutes, seconds)
                                            }
                                        }
                                        mysql::Value::NULL => "NULL".to_string(),
                                    }
                                }
                                Some(Err(_)) | None => "NULL".to_string(),
                            };
                            row_data.push(value);
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
        // First, try to get the current database
        let current_db_result: Vec<Row> = self.conn.query("SELECT DATABASE()")?;
        let current_db: Option<String> = if let Some(row) = current_db_result.first() {
            row.get::<Option<String>, _>(0).unwrap_or(None)
        } else {
            None
        };
        
        if let Some(db_name) = current_db {
            if !db_name.is_empty() && db_name != "information_schema" {
                // We have a current database, show tables
                let query = "SELECT COALESCE(TABLE_NAME, '') as table_name FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME IS NOT NULL";
                let result: Vec<Row> = self.conn.query(query)?;

                let mut tables = Vec::new();
                for row in result {
                    // Get the table name from our safe query
                    let table_name: String = row.get("table_name").unwrap_or_default();
                    if table_name.is_empty() {
                        continue; // Skip empty names
                    }

                    // Get row count for each table, with error handling
                    let count_query = format!("SELECT COUNT(*) FROM `{}`", table_name);
                    let count: Option<u64> = match self.conn.query_first(&count_query) {
                        Ok(c) => c,
                        Err(_) => None, // If we can't get count, just show None
                    };

                    tables.push(TableInfo {
                        name: table_name,
                        row_count: count.map(|c| c as usize),
                    });
                }
                Ok(tables)
            } else {
                // Empty database name, show databases
                let query = "SHOW DATABASES";
                let result: Vec<Row> = self.conn.query(query)?;

                let mut databases = Vec::new();
                for row in result {
                    let db_name: String = row.get(0).unwrap_or_default();
                    if !["information_schema", "mysql", "performance_schema", "sys"].contains(&db_name.as_str()) {
                        databases.push(TableInfo {
                            name: db_name,
                            row_count: None,
                        });
                    }
                }
                Ok(databases)
            }
        } else {
            // No current database, show available databases
            let query = "SELECT COALESCE(SCHEMA_NAME, '') as db_name FROM information_schema.SCHEMATA WHERE SCHEMA_NAME IS NOT NULL";
            let result: Vec<Row> = self.conn.query(query)?;

            let mut databases = Vec::new();
                for row in result {
                    // Get the database name from our safe query
                    let db_name: String = row.get("db_name").unwrap_or_default();
                    if db_name.is_empty() {
                        continue; // Skip empty names
                    }                // Skip system databases for cleaner display
                if !["information_schema", "mysql", "performance_schema", "sys"].contains(&db_name.as_str()) {
                    databases.push(TableInfo {
                        name: db_name,
                        row_count: None, // Don't calculate database sizes for now
                    });
                }
            }
            Ok(databases)
        }
    }

    fn get_table_columns(&mut self, _table_name: &str) -> Result<Vec<super::connection::ColumnInfo>> {
        // Not implemented yet
        Ok(Vec::new())
    }

    fn get_table_data(&mut self, table_name: &str, limit: usize, offset: usize) -> Result<QueryResult> {
        // Always get the current database from MySQL to ensure we have the right context
        let current_db = self.get_current_database()?;
        
        let qualified_table_name = if let Some(db_name) = current_db {
            if !db_name.is_empty() {
                // Use fully qualified table name
                format!("`{}`.`{}`", db_name, table_name)
            } else {
                return Err(anyhow::anyhow!("No database selected. Please select a database first."));
            }
        } else {
            return Err(anyhow::anyhow!("No database selected. Please select a database first."));
        };
        
        let query = format!("SELECT * FROM {} LIMIT {} OFFSET {}", qualified_table_name, limit, offset);
        self.execute_query(&query)
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

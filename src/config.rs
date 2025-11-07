use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionConfig {
    pub name: String,
    pub connection_string: String,
    #[serde(default)]
    pub db_type: String, // "sqlite", "mysql", "mariadb"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub connections: Vec<ConnectionConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let contents = fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(config_path, contents)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("tui-db").join("config.json"))
    }

    pub fn add_connection(&mut self, name: String, connection_string: String, db_type: String) {
        // Check if connection already exists, don't add duplicates
        if !self.connections.iter().any(|c| c.name == name && c.connection_string == connection_string) {
            self.connections.push(ConnectionConfig { name, connection_string, db_type });
        }
    }

    pub fn remove_connection(&mut self, name: &str) -> bool {
        let initial_len = self.connections.len();
        self.connections.retain(|c| c.name != name);
        self.connections.len() < initial_len
    }

    pub fn get_connections(&self) -> &[ConnectionConfig] {
        &self.connections
    }
}

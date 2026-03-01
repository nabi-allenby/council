use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Daemon configuration stored in config.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default)]
    pub daemon: DaemonSection,
    #[serde(default)]
    pub defaults: DefaultsSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSection {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsSection {
    #[serde(default = "default_rounds")]
    pub rounds: u32,
    #[serde(default = "default_min_participants")]
    pub min_participants: u32,
    #[serde(default = "default_join_timeout")]
    pub join_timeout: u32,
    #[serde(default = "default_turn_timeout")]
    pub turn_timeout: u32,
}

fn default_port() -> u16 {
    50051
}
fn default_host() -> String {
    "[::1]".to_string()
}
fn default_rounds() -> u32 {
    2
}
fn default_min_participants() -> u32 {
    2
}
fn default_join_timeout() -> u32 {
    60
}
fn default_turn_timeout() -> u32 {
    120
}

impl Default for DaemonSection {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
        }
    }
}

impl Default for DefaultsSection {
    fn default() -> Self {
        Self {
            rounds: default_rounds(),
            min_participants: default_min_participants(),
            join_timeout: default_join_timeout(),
            turn_timeout: default_turn_timeout(),
        }
    }
}

impl DaemonConfig {
    /// Returns the council config directory path.
    /// Uses `$XDG_CONFIG_HOME/council/` or `~/.config/council/`.
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("council"))
    }

    /// Returns the path to config.toml.
    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("config.toml"))
    }

    /// Returns the path to the PID file.
    pub fn pid_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("daemon.pid"))
    }

    /// Returns the path to the log file.
    pub fn log_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("daemon.log"))
    }

    /// Returns the path to the hooks directory.
    pub fn hooks_dir() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("hooks"))
    }

    /// Load config from the default path. Returns default config if file doesn't exist.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Write config to the default path.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("cannot determine config directory")?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// The daemon address as host:port.
    pub fn addr(&self) -> String {
        format!("{}:{}", self.daemon.host, self.daemon.port)
    }
}

// src/config/model.rs

use serde::Deserialize;
use std::{path::PathBuf, str::FromStr, time::Duration};
use thiserror::Error;

/// Top-level runtime config
#[derive(Debug)]
pub struct Config {
    pub logging:  LoggingConfig,
    pub database: DatabaseConfig,
    pub scanner:  Vec<RiskGroup>,
}

/// Mirror of the `[logging]` table
#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]            pub enable: bool,
    #[serde(default)]            pub file:   Option<String>,
    #[serde(default = "default_level")] pub level: String,
}
fn default_level() -> String { "INFO".into() }

/// Mirror of the `[database]` table — **no defaults**: must be present in TOML
#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub path:               String,
    pub purge_on_restart:   bool,
    pub synchronous:        String,
    pub journal_size_limit: u64,
    pub checkpoint_seconds: u64,
    pub ttl_seconds:        u64,
    pub flush_interval_ms:  u64,
    pub batch_size:         usize,
}

/// Holds the raw scanner entries from TOML
#[derive(Debug, Deserialize)]
pub struct RiskStub {
    pub risk:        String,
    #[serde(rename = "dirs")]
    pub directories: Vec<String>,
    #[serde(default)]
    pub interval:    Option<String>,
}

/// Fully-typed scanner group
#[derive(Debug, Clone)]
pub struct RiskGroup {
    pub risk:        DirectoryRisk,
    pub directories: Vec<PathBuf>,
    pub interval:    Option<Duration>,
}

/// Allowed risk levels; add a variant here to support new ones
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRisk {
    Low,
    Medium,
    High,
    Special,
}

/// All the ways config loading can go wrong
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid risk '{0}'")]
    InvalidRisk(String),

    #[error("invalid duration '{0}': {1}")]
    InvalidDuration(String, #[source] humantime::DurationError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Allow `"High"` → `DirectoryRisk::High"`
impl FromStr for DirectoryRisk {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low"     => Ok(DirectoryRisk::Low),
            "medium"  => Ok(DirectoryRisk::Medium),
            "high"    => Ok(DirectoryRisk::High),
            "special" => Ok(DirectoryRisk::Special),
            other     => Err(ConfigError::InvalidRisk(other.into())),
        }
    }
}

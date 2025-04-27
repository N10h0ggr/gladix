// src/config/types.rs

//! Configuration structures and risk‐level definitions.

use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

/// Per‐run logging settings.
#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    /// If true, write a session log file in addition to stdout.
    #[serde(default)]
    pub enable: bool,

    /// Relative path under the exe dir for the log file.
    #[serde(default)]
    pub file: Option<String>,

    /// Log level: "ERROR", "WARN", "INFO", "DEBUG", or "TRACE"
    #[serde(default = "default_level")]
    pub level: String,
}

fn default_level() -> String {
    "INFO".into()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            enable: false,
            file: None,
            level: default_level(),
        }
    }
}

/// Holds all scanner group settings.
#[derive(Debug, Deserialize, Clone)]
pub struct RiskGroupConfig {
    pub directories: Vec<String>,
    pub scheduled_interval: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum DirectoryRisk {
    Low,
    Medium,
    High,
    Special,
}

#[derive(Debug)]
pub struct RiskGroup {
    pub risk: DirectoryRisk,
    pub directories: Vec<PathBuf>,
    pub scheduled_interval: Option<Duration>,
}

#[derive(Debug, Deserialize)]
pub struct ScannerConfig {
    pub low: Option<RiskGroupConfig>,
    pub medium: Option<RiskGroupConfig>,
    pub high: Option<RiskGroupConfig>,
    pub special: Option<RiskGroupConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MasterConfig {
    pub scanner: ScannerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Default for MasterConfig {
    fn default() -> Self {
        MasterConfig {
            scanner: ScannerConfig {
                low: None,
                medium: None,
                high: None,
                special: None,
            },
            logging: LoggingConfig::default(),
        }
    }
}

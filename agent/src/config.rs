use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The four risks available.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DirectoryRisk {
    High,
    Medium,
    Low,
    Special,
}

/// A group of directories that share the same risk and scanning interval.
/// This is used at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskGroup {
    pub risk: DirectoryRisk,
    pub directories: Vec<PathBuf>,
    /// If Some(duration), the scan is scheduled with that interval;
    /// if None, then scans must be triggered manually.
    pub scheduled_interval: Option<Duration>,
}

/// Structures to be deserialized from the configuration file.

/// Configuration for a risk group from the config file.
/// The scheduled_interval is expressed in seconds.
#[derive(Debug, Serialize, Deserialize)]
pub struct RiskGroupConfig {
    pub scheduled_interval: Option<u64>,
    pub directories: Vec<String>,
}

/// General scanner configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub max_file_size_mb: u64,
    pub executable_extensions: Vec<String>,
}

/// All risk groups defined in the configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct RiskGroupsConfig {
    pub high: RiskGroupConfig,
    pub medium: RiskGroupConfig,
    pub low: RiskGroupConfig,
    pub special: RiskGroupConfig,
}

/// The master configuration file structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct MasterConfig {
    pub scanner: ScannerConfig,
    pub risk_groups: RiskGroupsConfig,
}

/// Loads the master configuration from a TOML file located at the given path.
pub fn load_master_config(path: &std::path::Path) -> Result<MasterConfig, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let config: MasterConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Converts a RiskGroupConfig (from the config file) into a runtime RiskGroup.
/// Directory strings are converted to PathBuf, and the scheduled_interval (if any)
/// is converted from seconds to a Duration.
pub fn convert_config_to_risk_group(risk: DirectoryRisk, config: RiskGroupConfig) -> RiskGroup {
    RiskGroup {
        risk,
        directories: config.directories.into_iter().map(PathBuf::from).collect(),
        scheduled_interval: config.scheduled_interval.map(Duration::from_secs),
    }
}

// src/config/loader.rs

//! # Configuration Loader
//!
//! Reads `default.toml`, deserializes into `MasterConfig`, and
//! converts raw risk‐group configs into runtime `RiskGroup`s.

use crate::gladix_log;
use crate::config::types::{MasterConfig, DirectoryRisk, RiskGroupConfig, RiskGroup};
use std::{fs, path::Path, path::PathBuf, time::Duration};

/// Load and parse the master configuration from `path`.
/// Logs at DEBUG before reading and INFO on success.
pub fn load_master_config(path: &Path) -> Result<MasterConfig, Box<dyn std::error::Error>> {
    gladix_log!(Level::Debug, "Reading config from {:?}", path);
    let txt = fs::read_to_string(path)?;
    let cfg: MasterConfig = toml::from_str(&txt)?;
    gladix_log!(Level::Info, "Loaded config from {:?}", path);
    Ok(cfg)
}

/// Convert one `RiskGroupConfig` into a runtime `RiskGroup`.
/// Logs at DEBUG with directory count and interval.
pub fn convert_config_to_risk_group(
    risk: DirectoryRisk,
    cfg: RiskGroupConfig,
) -> RiskGroup {
    let dirs: Vec<PathBuf> = cfg
        .directories
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let interval = cfg.scheduled_interval.map(Duration::from_secs);
    gladix_log!(
        Level::Debug,
        "Converted {:?} group: {} dirs, interval={:?}",
        risk,
        dirs.len(),
        interval
    );
    RiskGroup {
        risk,
        directories: dirs,
        scheduled_interval: interval,
    }
}

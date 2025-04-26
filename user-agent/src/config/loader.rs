//! # Configuration Loader
//!
//! Responsible for loading and parsing the master TOML config, and
//! converting raw risk‐group definitions into runtime scanner groups.
//!
//! **Responsibilities:**
//! - Read `default.toml` from disk.
//! - Deserialize into `MasterConfig`.
//! - Convert each `RiskGroupConfig` into a `RiskGroup` with `PathBuf`+`Duration`.

use crate::gladix_log;
use log::Level;
use crate::config::types::{MasterConfig, DirectoryRisk, RiskGroupConfig, RiskGroup};
use std::{fs, path::Path, path::PathBuf, time::Duration};

/// Load and parse the master configuration from `path`.
///
/// Logs at DEBUG when starting and INFO on success, or bubbles errors.
pub fn load_master_config(path: &Path) -> Result<MasterConfig, Box<dyn std::error::Error>> {
    gladix_log!(Level::Debug, "Reading config from {:?}", path);
    let txt = fs::read_to_string(path)?;
    let cfg: MasterConfig = toml::from_str(&txt)?;
    gladix_log!(Level::Info, "Loaded config from {:?}", path);
    Ok(cfg)
}

/// Convert a single `RiskGroupConfig` into a runtime `RiskGroup`.
///
/// Logs at DEBUG with the number of dirs and the scan interval.
pub fn convert_config_to_risk_group(
    risk: DirectoryRisk,
    cfg: RiskGroupConfig,
) -> RiskGroup {
    // Build PathBuf list and Duration
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

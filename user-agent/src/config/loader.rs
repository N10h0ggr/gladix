//! Configuration file loader and converter.
//!
//! This module handles loading and parsing of the main configuration file
//! (typically in TOML format). It transforms file-based structures into
//! runtime-ready formats with `PathBuf` and `Duration` types.
//!
//! Key responsibilities:
//! - Load and deserialize the master configuration from disk.
//! - Convert risk group definitions from raw config to executable form.
//! - Handle validation and format conversions safely.
//! - Provide a clean API to initialize scanning groups.


use crate::config::types::{
    MasterConfig,
    DirectoryRisk,
    RiskGroupConfig,
    RiskGroup,
};
use std::path::{Path, PathBuf};
use std::time::Duration;


/// Lee el TOML de disco y lo deserializa.
pub fn load_master_config(path: &Path) -> Result<MasterConfig, Box<dyn std::error::Error>> {
    let txt = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&txt)?)
}

/// Convierte un bloque RiskGroupConfig en el objeto de runtime.
pub fn convert_config_to_risk_group(risk: DirectoryRisk, cfg: RiskGroupConfig) -> RiskGroup {
    RiskGroup {
        risk,
        directories: cfg.directories
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        scheduled_interval: cfg
            .scheduled_interval
            .map(Duration::from_secs),
    }
}

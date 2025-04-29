// src/config/loader.rs

//! Reads `config.toml` into our `model::Config`

use crate::config::model::{
    Config, ConfigError, DatabaseConfig, DirectoryRisk,
    LoggingConfig, RiskGroup, RiskStub,
};
use humantime::parse_duration;
use std::{fs, path::Path, str::FromStr};

/// Entry point: read the file, parse, convert, validate.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    // 1. Read file (IO errors become ConfigError::Io)
    let text = fs::read_to_string(path)?;

    // 2. Raw deserialization (TOML errors become ConfigError::Toml)
    let raw: Raw = toml::from_str(&text)?;

    // 3. Convert to runtime types
    let mut groups = Vec::new();
    for stub in raw.scanner {
        // parse risk
        let risk = DirectoryRisk::from_str(&stub.risk)?;

        // parse directories
        let directories = stub
            .directories
            .into_iter()
            .map(|d| std::path::PathBuf::from(d))
            .collect();

        // parse optional human‐readable duration
        let interval = stub
            .interval
            .map(|s| parse_duration(&s).map_err(|e| ConfigError::InvalidDuration(s.clone(), e)))
            .transpose()?;

        groups.push(RiskGroup {
            risk,
            directories,
            interval,
        });
    }

    Ok(Config {
        logging:  raw.logging,
        database: raw.database,
        scanner:  groups,
    })
}

/// Mirrors the top-level TOML
#[derive(serde::Deserialize)]
struct Raw {
    pub logging:  LoggingConfig,
    pub database: DatabaseConfig,
    #[serde(rename = "scanner")]
    pub scanner:  Vec<RiskStub>,
}

//! Configuration structures and risk-level definitions.
//!
//! This module defines all configuration data structures used at both
//! deserialization and runtime stages of the agent. It distinguishes between
//! raw config formats (from TOML) and internal representations used during
//! execution.
//!
//! Key responsibilities:
//! - Define risk levels (`DirectoryRisk`) for directory groups.
//! - Represent runtime scan groups (`RiskGroup`).
//! - Define TOML-deserialized structures (`ScannerConfig`, `MasterConfig`, etc.).
//! - Keep a clean separation between config file format and logic-layer usage.
//! Configuration structures and risk-level definitions.
//!
//! This module defines all configuration data structures used at both
//! deserialization and runtime stages of the agent. It distinguishes between
//! raw config formats (from TOML) and internal representations used during
//! execution.
//!
//! Key responsibilities:
//! - Define risk levels (`DirectoryRisk`) for directory groups.
//! - Represent runtime scan groups (`RiskGroup`).
//! - Define TOML-deserialized structures (`ScannerConfig`, `MasterConfig`, etc.).
//! - Keep a clean separation between config file format and logic-layer usage.

use std::path::PathBuf;
use std::time::Duration;
use serde::Deserialize;

/// Risk levels for directory groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum DirectoryRisk {
    Low,
    Medium,
    High,
    Special,
}

/// Raw TOML block for each risk group.
#[derive(Debug, Deserialize)]
pub struct RiskGroupConfig {
    /// Paths (as strings) to scan under this risk level
    pub directories: Vec<String>,
    /// Optional interval (in seconds) between scans
    pub scheduled_interval: Option<u64>,
}

/// Runtime representation of a group to scan.
#[derive(Debug)]
pub struct RiskGroup {
    pub risk: DirectoryRisk,
    pub directories: Vec<PathBuf>,
    pub scheduled_interval: Option<Duration>,
}

/// Scanner section of your master config.
#[derive(Debug, Deserialize)]
pub struct ScannerConfig {
    pub low: Option<RiskGroupConfig>,
    pub medium: Option<RiskGroupConfig>,
    pub high: Option<RiskGroupConfig>,
    pub special: Option<RiskGroupConfig>, // <-- Add this
}

/// Top‐level config as deserialized from TOML.
#[derive(Debug, Deserialize)]
pub struct MasterConfig {
    pub scanner: ScannerConfig,
}

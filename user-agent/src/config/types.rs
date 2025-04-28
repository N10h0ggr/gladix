//! Strongly-typed configuration tree.
//
//  ┌─ MasterConfig ──┐
//  │ logging         │
//  │ database        │
//  │ scanner         │
//  └─────────────────┘
//
//  All defaults are sensible so the agent can run with an empty TOML.

use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

/* ───────────────────────────── LOGGING ──────────────────────────────── */

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    /// Write a session log file in addition to stdout.
    #[serde(default)]            pub enable: bool,
    /// Relative path for the log file (under the exe dir).
    #[serde(default)]            pub file: Option<String>,
    /// Level: ERROR / WARN / INFO / DEBUG / TRACE.
    #[serde(default = "d_level")]pub level: String,
}
fn d_level() -> String { "INFO".into() }
impl Default for LoggingConfig {
    fn default() -> Self { Self{ enable:false, file:None, level:d_level() } }
}

/* ───────────────────────────── SCANNER ──────────────────────────────── */

#[derive(Debug, Deserialize, Clone)]
pub struct RiskGroupConfig {
    pub directories: Vec<String>,
    pub scheduled_interval: Option<u64>,          // seconds
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum DirectoryRisk { Low, Medium, High, Special }

#[derive(Debug)]
pub struct RiskGroup {
    pub risk: DirectoryRisk,
    pub directories: Vec<PathBuf>,
    pub scheduled_interval: Option<Duration>,
}

#[derive(Debug, Deserialize)]
pub struct ScannerConfig {
    pub low:     Option<RiskGroupConfig>,
    pub medium:  Option<RiskGroupConfig>,
    pub high:    Option<RiskGroupConfig>,
    pub special: Option<RiskGroupConfig>,
}

/* ───────────────────────────── DATABASE ─────────────────────────────── */

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "d_path")]     pub path: String,
    #[serde(default)]                pub purge_on_restart: bool,
    #[serde(default = "d_sync")]     pub synchronous: String,      // FULL | NORMAL | OFF
    #[serde(default = "d_jsl")]      pub journal_size_limit: u64,  // bytes
    #[serde(default = "d_ckpt")]     pub checkpoint_seconds: u64,
    #[serde(default = "d_ttl")]      pub ttl_seconds: u64,
    #[serde(default = "d_flush")]    pub flush_interval_ms: u64,
    #[serde(default = "d_batch")]    pub batch_size: usize,
}
fn d_path()  -> String { "telemetry.db".into() }
fn d_sync()  -> String { "NORMAL".into() }
fn d_jsl()   -> u64    { 50_000_000 }
fn d_ckpt()  -> u64    { 600 }
fn d_ttl()   -> u64    { 86_400 }
fn d_flush() -> u64    { 250 }
fn d_batch() -> usize  { 1 }

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path:                d_path(),
            purge_on_restart:    false,
            synchronous:         d_sync(),
            journal_size_limit:  d_jsl(),
            checkpoint_seconds:  d_ckpt(),
            ttl_seconds:         d_ttl(),
            flush_interval_ms:   d_flush(),
            batch_size:          d_batch(),
        }
    }
}

/* ───────────────────────────── ROOT ─────────────────────────────────── */

#[derive(Debug, Deserialize)]
pub struct MasterConfig {
    pub scanner:  ScannerConfig,
    #[serde(default)] pub logging:  LoggingConfig,
    #[serde(default)] pub database: DatabaseConfig,
}

impl Default for MasterConfig {
    fn default() -> Self {
        Self {
            scanner:  ScannerConfig { low:None, medium:None, high:None, special:None },
            logging:  LoggingConfig::default(),
            database: DatabaseConfig::default(),
        }
    }
}

//! Windows service entry point for the EDR user-agent.
//!
//! This file serves as the main entry point of the user-agent executable,
//! which is designed to run as a Windows service. It initializes core modules,
//! loads configuration, sets up persistent state (such as file scan cache),
//! and launches background tasks including the static scanner scheduler.
//!
//! Key responsibilities:
//! - Load configuration and instantiate runtime structures.
//! - Prepare and manage persistent file scan cache.
//! - Launch scanning tasks for scheduled risk groups.
//! - Maintain long-running background execution as a service process.
//! - (Future scope) Integrate service registration, telemetry, and IPC.
//!
//! Note: At this stage, it blocks indefinitely to simulate a daemon/service.
//! Later, Windows service registration and graceful shutdown handling
//! should be added.


use std::path::PathBuf;
use agent::config::{load_master_config, convert_config_to_risk_group};
use agent::config::types::DirectoryRisk;
use agent::scanner::run_scanner;

fn main() {
    let master = load_master_config(PathBuf::from("agent_config.toml").as_path())
        .expect("Cannot read config");

    let risk_levels = [
        (DirectoryRisk::High,    master.scanner.high),
        (DirectoryRisk::Medium,  master.scanner.medium),
        (DirectoryRisk::Low,     master.scanner.low),
        (DirectoryRisk::Special, master.scanner.special),
    ];

    let groups: Vec<_> = risk_levels.into_iter()
        .filter_map(|(risk, opt_cfg)| opt_cfg.map(|cfg| convert_config_to_risk_group(risk, cfg)))
        .collect();

    let scheduled: Vec<_> = groups
        .into_iter()
        .filter(|g| g.scheduled_interval.is_some())
        .collect();

    run_scanner(scheduled, PathBuf::from("persistent_cache.json"));
}


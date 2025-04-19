mod cache;
mod config;
mod scanner;

use std::path::PathBuf;
use config::{load_master_config, convert_config_to_risk_group, DirectoryRisk};
use scanner::run_scanner;

fn main() {
    // Load the master configuration from a file.
    let config_path = PathBuf::from("agent_config.toml");
    let master_config = load_master_config(&config_path)
        .expect("Failed to load configuration file");

    // Convert each risk group configuration to a runtime RiskGroup.
    let high_group = convert_config_to_risk_group(DirectoryRisk::High, master_config.risk_groups.high);
    let medium_group = convert_config_to_risk_group(DirectoryRisk::Medium, master_config.risk_groups.medium);
    let _low_group = convert_config_to_risk_group(DirectoryRisk::Low, master_config.risk_groups.low);
    let _special_group = convert_config_to_risk_group(DirectoryRisk::Special, master_config.risk_groups.special);

    // Only include scheduled risk groups in run_scanner.
    let mut scheduled_groups = Vec::new();
    if high_group.scheduled_interval.is_some() {
        scheduled_groups.push(high_group);
    }
    if medium_group.scheduled_interval.is_some() {
        scheduled_groups.push(medium_group);
    }

    // Define the persistent cache file location.
    let persist_cache_file = PathBuf::from("persistent_cache.json");

    // Start the scanner module in scheduled mode.
    run_scanner(scheduled_groups, persist_cache_file);

    // Optionally, later add a trigger (GUI/CLI) for manual groups (low, special).
}

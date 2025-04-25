//! Windows service entrypoint for the Gladix User Agent Service.
//!
//! Installs as a SYSTEM service, handles startup/shutdown, loads config, initializes logging.

// Bring in the logging macro
#[macro_use]
mod macros;
mod scanner;
mod config;
mod comms;


use std::{ffi::OsString, path::PathBuf, sync::mpsc, thread, fs::File, process, time::Duration};
use chrono::Local;
use log::Level;
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use eventlog::init as event_init;
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher::start,
};

use crate::config::{load_master_config, convert_config_to_risk_group};
use crate::config::types::{MasterConfig, RiskGroup, DirectoryRisk};
use crate::scanner::run_scanner;

const SERVICE_NAME: &str = "Gladix";
const SERVICE_DISPLAY_NAME: &str = "Gladix User Agent Service";

define_windows_service!(ffi_service_main, service_main);

/// Configure logging: per-run file or Windows Event Log
fn setup_logging(exe_dir: &PathBuf, master: &MasterConfig) {
    if master.logging.enable {
        let log_file = exe_dir.join(
            master.logging.file.clone().unwrap_or_else(|| "session-debug.log".into()),
        );
        let config = ConfigBuilder::new()
            .set_time_offset_to_local().unwrap()
            .set_time_format_rfc3339()
            .set_thread_level(LevelFilter::Debug)
            .set_target_level(LevelFilter::Info)
            .set_location_level(LevelFilter::Off)
            .build();
        WriteLogger::init(
            LevelFilter::Debug,
            config,
            File::create(&log_file).unwrap_or_else(|e| {
                eprintln!(
                    "[{}][ERROR][logging][pid={}][tid={:?}] Could not create log file: {}",
                    Local::now().to_rfc3339(), process::id(), thread::current().id(), e
                );
                process::exit(1);
            }),
        ).unwrap_or_else(|e| {
            eprintln!(
                "[{}][ERROR][logging][pid={}][tid={:?}] Failed to initialize file logger: {}",
                Local::now().to_rfc3339(), process::id(), thread::current().id(), e
            );
            process::exit(1);
        });
        gladix_log!(Level::Info, "logging", "Log file: {:?}", log_file);
    } else {
        event_init(SERVICE_DISPLAY_NAME, Level::Info)
            .unwrap_or_else(|e| eprintln!(
                "[{}][ERROR][logging][pid={}][tid={:?}] Event log init failed: {}",
                Local::now().to_rfc3339(), process::id(), thread::current().id(), e
            ));
    }
}

/// Main service logic
fn run_service() {
    // Determine executable directory
    let exe_dir = std::env::current_exe()
        .expect("Failed to get exe path")
        .parent().expect("Executable must reside in a directory").to_path_buf();

    gladix_log!(Level::Info, "service", "Test message");
    
    // Load configuration
    let config_path = exe_dir.join("default.toml");
    let master: MasterConfig = load_master_config(&config_path)
        .unwrap_or_else(|e| {
            eprintln!(
                "[{}][ERROR][config][pid={}][tid={:?}] Config load failed: {}",
                Local::now().to_rfc3339(), process::id(), thread::current().id(), e
            );
            process::exit(1);
        });

    // Initialize logging
    setup_logging(&exe_dir, &master);
    gladix_log!(Level::Info, "service", "Service starting");

    // Register control handler
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                gladix_log!(Level::Info, "service", "Stop requested");
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    ).expect("Failed to register service control handler");

    // Set service status to StartPending
    let mut status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    };
    status_handle.set_service_status(status.clone()).unwrap();

    // Prepare scanning groups
    let groups: Vec<RiskGroup> = [
        (DirectoryRisk::High, master.scanner.high),
        (DirectoryRisk::Medium, master.scanner.medium),
        (DirectoryRisk::Low, master.scanner.low),
        (DirectoryRisk::Special, master.scanner.special),
    ].into_iter()
        .filter_map(|(risk, opt)| opt.map(|cfg| convert_config_to_risk_group(risk, cfg)))
        .filter(|g| g.scheduled_interval.is_some())
        .collect();

    // Set service state to Running
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    gladix_log!(Level::Info, "service", "Service running with {} group(s)", groups.len());

    // Launch scanner thread
    let cache_path = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache_path));

    // Wait for stop signal
    let _ = shutdown_rx.recv();
    gladix_log!(Level::Info, "service", "Shutdown initiated");

    // Mark service as stopped
    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    gladix_log!(Level::Info, "service", "Service stopped cleanly");
}

/// Entry point for the Windows Service control dispatcher
fn service_main(_args: Vec<OsString>) {
    run_service();
}

/// Program entry: attempt to run as service; fallback to console
fn main() {
    if let Err(e) = start(SERVICE_NAME, ffi_service_main) {
        eprintln!(
            "[{}][ERROR][main][pid={}][tid={:?}] Not a service: {}. Running in console mode.",
            Local::now().to_rfc3339(), process::id(), thread::current().id(), e
        );
        run_service();
    }
}

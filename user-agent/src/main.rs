// src/main.rs

//! Agent entrypoint: Windows-service or console fallback.

#[macro_use]
mod macros;
mod config;
mod scanner;
mod comms;

use chrono::Local;
use log::LevelFilter;
use simplelog::{CombinedLogger, TermLogger, WriteLogger, ConfigBuilder, TerminalMode, ColorChoice};
use std::{ffi::OsString, fs::File, path::PathBuf, process, sync::mpsc, thread, time::Duration};
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

define_windows_service!(ffi_service_main, service_main);


fn setup_logging(exe_dir: &PathBuf, master: &MasterConfig) {
    // 1) Read desired level from config
    let level = match master.logging.level.to_uppercase().as_str() {
        "ERROR" => LevelFilter::Error,
        "WARN"  => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        "OFF"   => LevelFilter::Off,
        _       => LevelFilter::Info,
    };

    // 2) Turn off all of simplelog’s own stamping (time/level/target/thread/location)
    let config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Off)        // no simplelog [LEVEL]
        .set_time_format_rfc3339()              // RFC3339 if we did use it
        .set_time_level(LevelFilter::Off)
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Off)
        .build();

    // 3) Always log to console
    let mut backends: Vec<Box<dyn simplelog::SharedLogger>> = vec![
        TermLogger::new(level, config.clone(), TerminalMode::Mixed, ColorChoice::Never),
    ];

    // 4) Optionally also to a file
    if master.logging.enable {
        let log_path = exe_dir.join(master.logging.file.clone().unwrap_or_else(|| "agent.log".into()));
        let f = File::create(&log_path).unwrap_or_else(|e| {
            eprintln!("[{}][ERROR][logging] Could not open {}: {}", Local::now().to_rfc3339(), log_path.display(), e);
            process::exit(1);
        });
        backends.push(WriteLogger::new(level, config, f));
    }

    CombinedLogger::init(backends).unwrap_or_else(|e| {
        eprintln!("[{}][ERROR][logging] Logger init failed: {}", Local::now().to_rfc3339(), e);
        process::exit(1);
    });

    // Our kick‐off message
    gladix_log!(Level::Info, "Logging initialized (level={:?})", level);
}


fn run_service() {
    // Locate the executable directory for config and cache files
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // Load and validate the master configuration file (default.toml)
    let master: MasterConfig = load_master_config(&exe_dir.join("default.toml")).unwrap_or_else(|e| {
        eprintln!(
            "[{}][ERROR][config] Config load failed: {}",
            Local::now().to_rfc3339(),
            e
        );
        process::exit(1);
    });

    setup_logging(&exe_dir, &master);
    gladix_log!(log::Level::Info, "Service starting");

    // Use a sync channel with capacity=1 to signal a single shutdown event
    let (tx, rx) = mpsc::sync_channel(1);

    // Register handler for Windows service control events (Stop/Shutdown)
    let status_handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            // Notify the main thread to begin clean shutdown
            gladix_log!(log::Level::Warn, "Stop requested");
            let _ = tx.send(());
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    }).unwrap();

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

    // Build only those risk groups that have a scheduled scan interval
    // This avoids spawning scanner threads for disabled groups
    let groups: Vec<RiskGroup> = vec![
        (DirectoryRisk::High,   master.scanner.high),
        (DirectoryRisk::Medium, master.scanner.medium),
        (DirectoryRisk::Low,    master.scanner.low),
        (DirectoryRisk::Special,master.scanner.special),
    ]
        .into_iter()
        .filter_map(|(risk, opt_config)| {
            // Convert only configured entries into RiskGroup instances
            opt_config.map(|cfg| convert_config_to_risk_group(risk, cfg))
        })
        .filter(|group| group.scheduled_interval.is_some())
        .collect();

    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    gladix_log!(log::Level::Info, "Service running with {} group(s)", groups.len());

    let cache_path = exe_dir.join("persistent_cache.json");

    // Run the scanner on a background thread so the service control loop stays responsive
    thread::spawn(move || run_scanner(groups, cache_path));

    // Block until a shutdown signal is received
    let _ = rx.recv();
    gladix_log!(log::Level::Warn, "Shutdown initiated");

    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    gladix_log!(log::Level::Info, "Service stopped cleanly");
}

fn service_main(_args: Vec<OsString>) {
    run_service();
}

fn main() {
    // Attempt to run as a Windows service; fallback to console if not running under SCM
    if let Err(e) = start(SERVICE_NAME, ffi_service_main) {
        eprintln!(
            "[{}][ERROR][main] Not a service: {}. Running as console.",
            Local::now().to_rfc3339(),
            e
        );
        run_service();
    }
}

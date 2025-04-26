//! Windows service + console entrypoint for the Gladix User Agent.

#[macro_use]
mod macros;
mod scanner;
mod config;
mod comms;

use chrono::Local;
use log::{Level, LevelFilter};
use simplelog::{CombinedLogger, TermLogger, WriteLogger, ConfigBuilder, TerminalMode, ColorChoice};
use std::{ffi::OsString, fs::File, path::PathBuf, process, sync::mpsc, thread, time::Duration};

#[cfg(windows)]
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher::start,
};
#[cfg(windows)]
use eventlog::init as event_init;

// No-op stubs so we still build on non-Windows:
#[cfg(not(windows))]
macro_rules! define_windows_service { ($($t:tt)*) => {} }
#[cfg(not(windows))]
fn start(_name: &str, _f: fn(Vec<OsString>)) -> Result<(), ()> { Ok(()) }
#[cfg(not(windows))]
fn event_init(_disp: &str, _lvl: log::Level) -> Result<(), ()> { Ok(()) }

use crate::config::{load_master_config, convert_config_to_risk_group};
use crate::config::types::{MasterConfig, RiskGroup, DirectoryRisk};
use crate::scanner::run_scanner;

const SERVICE_NAME: &str = "Gladix";
const SERVICE_DISPLAY_NAME: &str = "Gladix User Agent Service";

define_windows_service!(ffi_service_main, service_main);

/// Set up our combined logger: always stdout + optional file.
fn setup_logging(exe_dir: &PathBuf, master: &MasterConfig) {
    // 1) Parse the level string into LevelFilter
    let level = match master.logging.level.to_uppercase().as_str() {
        "OFF" => LevelFilter::Off,
        "ERROR" => LevelFilter::Error,
        "WARN" | "WARNING" => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    // 2) Determine the log file path
    let log_file = exe_dir.join(
        master.logging.file.clone().unwrap_or_else(|| "agent.log".into()),
    );

    // 3) Build logger collection
    let mut loggers: Vec<Box<dyn simplelog::SharedLogger>> = Vec::new();

    // Always log to stdout (no ANSI colors)
    loggers.push(TermLogger::new(
        level,
        ConfigBuilder::new()
            .set_time_format_rfc3339()
            .build(),
        TerminalMode::Mixed,
        ColorChoice::Never,
    ));

    // If enabled, also write to a file (overwriting on each run)
    if master.logging.enable {
        let f = File::create(&log_file).unwrap_or_else(|e| {
            eprintln!(
                "[{}][ERROR][logging][pid={}][tid={:?}] Could not open {}: {}",
                Local::now().to_rfc3339(),
                process::id(),
                thread::current().id(),
                log_file.display(),
                e
            );
            process::exit(1);
        });
        loggers.push(WriteLogger::new(
            level,
            ConfigBuilder::new()
                .set_time_format_rfc3339()
                .build(),
            f,
        ));
    }

    // 4) Initialize the CombinedLogger
    CombinedLogger::init(loggers).unwrap_or_else(|e| {
        eprintln!(
            "[{}][ERROR][logging][pid={}][tid={:?}] Logger setup failed: {}",
            Local::now().to_rfc3339(),
            process::id(),
            thread::current().id(),
            e
        );
        process::exit(1);
    });

    // 5) Kick off with an INFO so we know logging’s alive
    gladix_log!(
        Level::Info,
        "Logging initialized (level={:?}, file={})",
        level,
        log_file.display()
    );
}

/// The core service (or console) workflow
fn run_service() {
    // Find the executable directory
    let exe_dir = std::env::current_exe()
        .expect("Failed to get exe path")
        .parent()
        .expect("Exe must be in a directory")
        .to_path_buf();

    // Load default.toml
    let config_path = exe_dir.join("default.toml");
    let master: MasterConfig = load_master_config(&config_path).unwrap_or_else(|e| {
        eprintln!(
            "[{}][ERROR][config][pid={}][tid={:?}] Config load failed: {}",
            Local::now().to_rfc3339(),
            process::id(),
            thread::current().id(),
            e
        );
        process::exit(1);
    });

    // Initialize logging
    setup_logging(&exe_dir, &master);
    gladix_log!(Level::Info, "Service starting");

    // Register Ctrl handler for Stop/Shutdown
    let (tx, rx) = mpsc::sync_channel(1);
    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |evt| match evt {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                gladix_log!(Level::Warn, "Stop requested");
                let _ = tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    )
        .expect("Failed to register service control handler");

    // Report StartPending → Running
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

    // Build scanner groups from config
    let groups: Vec<RiskGroup> = [
        (DirectoryRisk::High, master.scanner.high),
        (DirectoryRisk::Medium, master.scanner.medium),
        (DirectoryRisk::Low, master.scanner.low),
        (DirectoryRisk::Special, master.scanner.special),
    ]
        .into_iter()
        .filter_map(|(risk, opt)| opt.map(|c| convert_config_to_risk_group(risk, c)))
        .filter(|g| g.scheduled_interval.is_some())
        .collect();

    // Now running
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    gladix_log!(Level::Info, "Service running with {} group(s)", groups.len());

    // Spawn scanner thread
    let cache = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache));

    // Block until Stop/Shutdown
    let _ = rx.recv();
    gladix_log!(Level::Warn, "Shutdown initiated");

    // Mark Stopped
    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    gladix_log!(Level::Info, "Service stopped cleanly");
}

/// Called by the Windows service dispatcher
fn service_main(_args: Vec<OsString>) {
    run_service();
}

/// Entry point: service mode or console fallback
fn main() {
    if let Err(_) = start(SERVICE_NAME, ffi_service_main) {
        eprintln!(
            "[{}][ERROR][main][pid={}][tid={:?}] Not a service, running console",
            Local::now().to_rfc3339(),
            process::id(),
            thread::current().id()
        );
        run_service();
    }
}

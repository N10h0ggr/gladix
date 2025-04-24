//! Windows service entry point for the Gladix User Agent Service.
//!
//! Installable as SYSTEM service, handles graceful startup/shutdown, loads config,
//! initializes modules, and logs lifecycle to the Windows Event Log.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use log::{error, info};
use eventlog::{init, register, deregister};
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
    ServiceType, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceAccess,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher::start;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use agent::config::{load_master_config, convert_config_to_risk_group};
use agent::config::types::{MasterConfig, RiskGroup, DirectoryRisk};
use agent::scanner::run_scanner;

const SERVICE_NAME: &str = "Gladix";
const SERVICE_DISPLAY_NAME: &str = "Gladix User Agent Service";

define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<OsString>) {
    // Initialize Windows Event Log
    init(SERVICE_DISPLAY_NAME, log::Level::Info)
        .unwrap_or_else(|e| eprintln!("eventlog init failed: {}", e));

    // Channel for shutdown signal
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel::<()>(1);

    // Register service control handler
    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                info!("Stop requested");
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    )
    .expect("Failed to register service control handler");

    // Report Start Pending
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
    info!("Service starting");

    // Determine executable directory
    let exe_dir = std::env::current_exe()
        .expect("Failed to get exe path")
        .parent()
        .expect("Executable must reside in a directory")
        .to_path_buf();

    // Load configuration
    let config_path = exe_dir.join("default.toml");
    let master: MasterConfig = load_master_config(&config_path)
        .unwrap_or_else(|e| { error!("Failed to load config: {}", e); std::process::exit(1) });
    info!("Configuration loaded from {:?}", config_path);

    // Prepare scheduled risk groups
    let groups: Vec<RiskGroup> = [
        (DirectoryRisk::High, master.scanner.high),
        (DirectoryRisk::Medium, master.scanner.medium),
        (DirectoryRisk::Low, master.scanner.low),
        (DirectoryRisk::Special, master.scanner.special),
    ]
    .into_iter()
    .filter_map(|(risk, opt)| opt.map(|cfg| convert_config_to_risk_group(risk, cfg)))
    .filter(|g| g.scheduled_interval.is_some())
    .collect();

    // Report Running
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    info!("Service running with {} scheduled group(s)", groups.len());

    // Launch scanner thread
    let cache_path = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache_path));

    // Wait for stop signal
    let _ = shutdown_rx.recv();
    info!("Shutdown initiated");

    // Report Stopped
    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    info!("Service stopped cleanly");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Uninstall command
    if args.iter().any(|a| a.eq_ignore_ascii_case("uninstall")) {
        deregister(SERVICE_DISPLAY_NAME)
            .unwrap_or_else(|e| eprintln!("Event source deregister failed: {}", e));
        let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .expect("Failed to connect to SCM");
        let svc = mgr.open_service(
            OsString::from(SERVICE_NAME),
            ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
        ).expect("Failed to open service");
        svc.delete().expect("Failed to delete service");
        info!("Service uninstalled: {}", SERVICE_DISPLAY_NAME);
        println!("{} uninstalled successfully.", SERVICE_DISPLAY_NAME);
        return;
    }

    // Install command
    if args.iter().any(|a| a.eq_ignore_ascii_case("install")) {
        register(SERVICE_DISPLAY_NAME)
            .unwrap_or_else(|e| eprintln!("Event source register failed: {}", e));
        let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)
            .expect("Failed to connect to SCM");
        let exe_path = std::env::current_exe().expect("Failed to get exe path");
        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path,
            launch_arguments: vec![],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        mgr.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
            .expect("Failed to create service");
        info!("Service installed: {}", SERVICE_DISPLAY_NAME);
        println!("{} installed successfully.", SERVICE_DISPLAY_NAME);
        return;
    }

    // Run as a Windows service
    start(SERVICE_NAME, ffi_service_main).expect("Failed to start service dispatcher");
}

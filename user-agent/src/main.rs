// src/main.rs

//! Agent entry-point: Windows service or console fallback.
//!
//! 1. Parse configuration & set up structured logging
//! 2. Initialise SQLite (WAL/NORMAL) and spawn async writers
//! 3. Register with the SCM (or run as a console fallback)
//! 4. Launch the directory scanner in a blocking thread
//! 5. Wait for Stop / Shutdown, then exit cleanly

mod comms;
mod config;
mod db;
mod scanner;

use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process,
    sync::mpsc,
    thread,
    time::Duration,
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc as async_mpsc;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher::start,
};

use config::{load_master_config, convert_config_to_risk_group};
use config::types::{MasterConfig, RiskGroup, DirectoryRisk};
use comms::events::{EtwEvent, FileEvent, NetworkEvent};
use db::{connection::init_database, connection::open_db_connection, maintenance::{spawn_ttl_cleanup, spawn_wal_maintenance}, spawn_writer};

use metrics_exporter_prometheus::PrometheusBuilder;
use scanner::run_scanner;

const SERVICE_NAME: &str = "Gladix";

define_windows_service!(ffi_service_main, service_main);

macro_rules! fatal {
    ($ctx:expr, $($arg:tt)+) => {{
        eprintln!(
            "[{}][ERROR][{}] {}",
            Local::now().to_rfc3339(),
            $ctx,
            format!($($arg)+)
        );
        std::process::exit(1);
    }};
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .expect("Cannot determine exe path")
        .parent()
        .expect("Executable must live in some directory")
        .to_path_buf()
}

fn load_master_cfg(exe_dir: &Path) -> MasterConfig {
    load_master_config(&exe_dir.join("default.toml"))
        .unwrap_or_else(|e| fatal!("config", "{}", e))
}


fn setup_logging(exe_dir: &Path, master: &MasterConfig) -> Result<(), fern::InitError> {
    let level = match master.logging.level.to_uppercase().as_str() {
        "ERROR" => LevelFilter::Error,
        "WARN"  => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        _       => LevelFilter::Info,
    };
    let log_path = master
        .logging
        .enable
        .then(|| exe_dir.join(master.logging.file.as_deref().unwrap_or("agent.log")));

    let mut dispatch = Dispatch::new()
        .format(|out, msg, record| {
            out.finish(format_args!(
                "[{}][{:5}][{}][pid={}][tid={:?}] {}",
                Local::now().to_rfc3339(),
                record.level(),
                record.target(),
                process::id(),
                thread::current().id(),
                msg
            ))
        })
        .level(level)
        .chain(std::io::stdout());

    if let Some(path) = log_path {
        dispatch = dispatch.chain(fern::log_file(path)?);
    }

    dispatch.apply()?;
    Ok(())
}

fn build_scanner_groups(master: &MasterConfig) -> Vec<RiskGroup> {
    [
        (DirectoryRisk::High,   &master.scanner.high),
        (DirectoryRisk::Medium, &master.scanner.medium),
        (DirectoryRisk::Low,    &master.scanner.low),
        (DirectoryRisk::Special,&master.scanner.special),
    ]
        .into_iter()
        .filter_map(|(risk, cfg)| cfg.as_ref().map(|c| convert_config_to_risk_group(risk, c.clone())))
        .filter(|g| g.scheduled_interval.is_some())
        .collect()
}


fn run_service() {
    // Context & config
    let exe    = exe_dir();
    let master = load_master_cfg(&exe);
    
    // Logging
    setup_logging(&exe, &master).expect("Logging setup failed");
    log::info!("Service bootstrap initiated");

    // Metrics endpoint
    let _recorder = PrometheusBuilder::new().install();

    // Database
    let db_cfg  = &master.database;
    let db_path = db::connection::db_path(&exe, db_cfg);

    let conn_file = init_database(&exe, db_cfg).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_net  = open_db_connection(&db_path, db_cfg).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_etw  = open_db_connection(&db_path, db_cfg).unwrap_or_else(|e| fatal!("database", "{}", e));


    // Tokio & DB writers
    let rt = Runtime::new().expect("Tokio runtime failed");
    let (_file_tx, file_rx) = async_mpsc::channel::<FileEvent>(10_000);
    let (_net_tx,  net_rx ) = async_mpsc::channel::<NetworkEvent>(10_000);
    let (_etw_tx,  etw_rx ) = async_mpsc::channel::<EtwEvent>(10_000);
    
    // when spawning writers
    spawn_writer(&rt, conn_file, file_rx, db_cfg);
    spawn_writer(&rt, conn_net,  net_rx,  db_cfg);
    spawn_writer(&rt, conn_etw,  etw_rx,  db_cfg);

    // background tasks
    spawn_ttl_cleanup(&rt, db_path.clone(), db_cfg);
    spawn_wal_maintenance(&rt, db_path.clone(), db_cfg);

    // Windows SCM
    let (svc_tx, svc_rx) = mpsc::sync_channel(1);
    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |ctrl| match ctrl {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                log::warn!("Stop requested via SCM");
                let _ = svc_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    ).unwrap();

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

    // Scanner thread
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    let groups     = build_scanner_groups(&master);
    log::info!("Service running with {} scanner groups", groups.len());
    let cache_path = exe.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache_path));

    // Shutdown
    let _ = svc_rx.recv();
    log::warn!("Shutdown initiated");
    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    log::info!("Service stopped cleanly");
}

fn service_main(_args: Vec<OsString>) {
    run_service();
}

fn main() {
    if start(SERVICE_NAME, ffi_service_main).is_err() {
        eprintln!(
            "[{}][ERROR][main] Not a service; falling back to console.",
            Local::now().to_rfc3339()
        );
        run_service();
    }
}

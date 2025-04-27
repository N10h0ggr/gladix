// src/main.rs

//! Agent entry-point: Windows service or console fallback.
//!
//! 1. Parse configuration & set up structured logging
//! 2. Initialise SQLite (WAL/NORMAL) and spawn async writers
//! 3. Register with the SCM (or run as a console fallback)
//! 4. Launch the directory scanner in a blocking thread
//! 5. Wait for Stop / Shutdown, then exit cleanly
//!
//!
// ───── project modules ──────────────────────────────────────────────────────
mod comms;
mod config;
mod db;
mod scanner;

// ───── std / 3rd-party imports ──────────────────────────────────────────────
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use rusqlite::Connection;
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

// ───── local imports ────────────────────────────────────────────────────────
use crate::comms::events::{EtwEvent, FileEvent, NetworkEvent};
use crate::config::{convert_config_to_risk_group, load_master_config};
use crate::config::types::{DirectoryRisk, MasterConfig, RiskGroup};
use crate::db::db_writer::{BatchInsert, DbWriter};
use crate::scanner::run_scanner;

// ───── service constants ────────────────────────────────────────────────────
const SERVICE_NAME: &str = "Gladix";

define_windows_service!(ffi_service_main, service_main);

// ───── helpers ──────────────────────────────────────────────────────────────

/// Print an error with context and terminate the process.
macro_rules! fatal {
    ($ctx:expr, $($arg:tt)+) => {{
        eprintln!(
            "[{}][ERROR][{}] {}",
            chrono::Local::now().to_rfc3339(),
            $ctx,
            format!($($arg)+)
        );
        std::process::exit(1);
    }};
}

/// Directory that contains the running executable.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .expect("Cannot determine exe path")
        .parent()
        .expect("Executable must live in some directory")
        .to_path_buf()
}

/// Load `default.toml` next to the executable.
fn load_master_cfg(exe_dir: &Path) -> MasterConfig {
    load_master_config(&exe_dir.join("default.toml"))
        .unwrap_or_else(|e| fatal!("config", "{}", e))
}

/// Open a SQLite database in WAL mode with NORMAL sync.
fn open_db_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", &"WAL")?;
    conn.pragma_update(None, "synchronous", &"NORMAL")?;
    Ok(conn)
}

/// Initialise the main database and apply the schema if it is the first run.
fn init_database(db_path: &Path) -> rusqlite::Result<Connection> {
    let first_run = !db_path.exists();
    let conn = open_db_connection(db_path)?;
    conn.pragma_update(None, "journal_size_limit", &50_000_000_i64)?;
    if first_run {
        let schema = include_str!("../resources/schema.sql");
        conn.execute_batch(schema)?;
    }
    log::info!("Database ready at {}", db_path.display());
    Ok(conn)
}


/// Configure global logging as requested in `master.logging`.
fn setup_logging(exe_dir: &Path, master: &MasterConfig) -> Result<(), fern::InitError> {
    let level = match master.logging.level.to_uppercase().as_str() {
        "ERROR" => LevelFilter::Error,
        "WARN" => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        _ => LevelFilter::Info,
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
                record.target(), // Only print the target (module path)
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

/// Spawn an asynchronous `DbWriter` for the given event type.
fn spawn_writer<E>(rt: &Runtime, conn: Connection, rx: async_mpsc::Receiver<E>)
where
    E: BatchInsert<E> + Send + 'static,
{
    rt.spawn(async move {
        DbWriter::<E> {
            conn,
            rx,
            flush_interval_ms: 250,
            batch_size: 1_000,
        }
            .run()
            .await;
    });
}

/// Convert configured directory-risk sections into scanner groups.
fn build_scanner_groups(master: &MasterConfig) -> Vec<RiskGroup> {
    [
        (DirectoryRisk::High, &master.scanner.high),
        (DirectoryRisk::Medium, &master.scanner.medium),
        (DirectoryRisk::Low, &master.scanner.low),
        (DirectoryRisk::Special, &master.scanner.special),
    ]
        .into_iter()
        .filter_map(|(risk, cfg)| {
            cfg.as_ref()
                .map(|c| convert_config_to_risk_group(risk, c.clone()))
        })
        .filter(|g| g.scheduled_interval.is_some())
        .collect()
}

// ───── service logic ────────────────────────────────────────────────────────

fn run_service() {
    // 1 ─ Context
    let exe_dir = exe_dir();
    let master = load_master_cfg(&exe_dir);

    // 2 ─ Logging
    setup_logging(&exe_dir, &master).expect("Logging setup failed");
    log::info!("Service bootstrap initiated");

    // 3 ─ Database
    let db_path = exe_dir.join("telemetry.db");
    let conn_file = init_database(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_net = open_db_connection(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_etw = open_db_connection(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));

    // 4 ─ Tokio runtime & DB writers
    let rt = Runtime::new().expect("Tokio runtime creation failed");

    let (_file_tx, file_rx) = async_mpsc::channel::<FileEvent>(10_000);
    let (_net_tx, net_rx) = async_mpsc::channel::<NetworkEvent>(10_000);
    let (_etw_tx, etw_rx) = async_mpsc::channel::<EtwEvent>(10_000);

    spawn_writer(&rt, conn_file, file_rx);
    spawn_writer(&rt, conn_net, net_rx);
    spawn_writer(&rt, conn_etw, etw_rx);

    // TODO: wire *_tx into the comms subsystem.

    // 5 ─ Windows SCM registration
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
    )
        .unwrap();

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

    // 6 ─ Scanner
    let scanner_groups = build_scanner_groups(&master);
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();
    log::info!("Service running with {} scanner group(s)", scanner_groups.len());

    let cache_path = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(scanner_groups, cache_path));

    // 7 ─ Wait for shutdown
    let _ = svc_rx.recv();
    log::warn!("Shutdown initiated");

    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    log::info!("Service stopped cleanly");
}

// Windows SCM entry point.
fn service_main(_args: Vec<OsString>) {
    run_service();
}

fn main() {
    // If registering as a service fails, run as a plain console app.
    if let Err(e) = start(SERVICE_NAME, ffi_service_main) {
        eprintln!(
            "[{}][ERROR][main] Not a service: {} – falling back to console.",
            Local::now().to_rfc3339(),
            e
        );
        run_service();
    }
}

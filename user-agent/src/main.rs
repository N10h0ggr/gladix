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

use config::{convert_config_to_risk_group, load_master_config};
use config::types::{DirectoryRisk, MasterConfig, RiskGroup};
use comms::events::{EtwEvent, FileEvent, NetworkEvent};
use db::db_writer::{BatchInsert, DbWriter};
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

fn open_db_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", &"WAL")?;
    conn.pragma_update(None, "synchronous", &"NORMAL")?;
    Ok(conn)
}

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

fn spawn_ttl_cleanup(rt: &Runtime, db_path: PathBuf, ttl_seconds: i64) {
    let one_hour = Duration::from_secs(3600);
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(one_hour);
        loop {
            ticker.tick().await;
            if let Ok(conn) = Connection::open(&db_path) {
                let cutoff = chrono::Utc::now().timestamp() - ttl_seconds;
                let _ = conn.execute("DELETE FROM fs_events    WHERE ts < ?1", [cutoff]);
                let _ = conn.execute("DELETE FROM network_events WHERE ts < ?1", [cutoff]);
                let _ = conn.execute("DELETE FROM etw_events    WHERE ts < ?1", [cutoff]);
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                log::info!("TTL cleanup: removed events before {}", cutoff);
            }
        }
    });
}

fn spawn_wal_maintenance(rt: &Runtime, db_path: PathBuf) {
    let ten_min = Duration::from_secs(600);
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(ten_min);
        loop {
            ticker.tick().await;
            if let Ok(conn) = Connection::open(&db_path) {
                if let Ok(mode) = conn.query_row("PRAGMA journal_mode;", [], |r| r.get::<_, String>(0)) {
                    log::debug!("PRAGMA journal_mode = {}", mode);
                }
                if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
                    log::warn!("WAL checkpoint failed: {}", e);
                }
            }
        }
    });
}

fn run_service() {
    // Context & config
    let exe = exe_dir();
    let master = load_master_cfg(&exe);

    // Logging
    setup_logging(&exe, &master).expect("Logging setup failed");
    log::info!("Service bootstrap initiated");

    // Metrics endpoint
    let _recorder = PrometheusBuilder::new().install();

    // Database
    let db_path   = exe.join("telemetry.db");
    let conn_file = init_database(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_net  = open_db_connection(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));
    let conn_etw  = open_db_connection(&db_path).unwrap_or_else(|e| fatal!("database", "{}", e));

    // Tokio & DB writers
    let rt = Runtime::new().expect("Tokio runtime failed");
    let (_file_tx, file_rx) = async_mpsc::channel::<FileEvent>(10_000);
    let (_net_tx,  net_rx ) = async_mpsc::channel::<NetworkEvent>(10_000);
    let (_etw_tx,  etw_rx ) = async_mpsc::channel::<EtwEvent>(10_000);

    spawn_writer(&rt, conn_file, file_rx);
    spawn_writer(&rt, conn_net,  net_rx);
    spawn_writer(&rt, conn_etw,  etw_rx);

    // TTL & WAL maintenance
    let ttl_secs = 7 * 24 * 3600; // 7 days
    spawn_ttl_cleanup(&rt, db_path.clone(), ttl_secs);
    spawn_wal_maintenance(&rt, db_path.clone());

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

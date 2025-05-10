// src/main.rs
//! Gladix user‑agent – Windows service or console app.
//
//  Execution flow
//  ──────────────────────────────────────────────────────────────────
//  1. Load configuration (embedded defaults ← TOML ← env).
//  2. Initialise structured logging + Prometheus registry.
//  3. Open / migrate SQLite; spawn async writers + maintenance jobs.
//  4. Start ring‑buffer router (reads BaseEvent blobs from the driver).
//  5. Register with Windows SCM (or fall back to console).
//  6. Spawn directory scanner on its own blocking thread.
//  7. Wait for Ctrl‑C or SCM “Stop” request → graceful shutdown.

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
    sync::{mpsc},
    thread,
    time::Duration,
};
use tokio::{runtime::Runtime, sync::{broadcast, mpsc as async_mpsc}};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher::start,
};

use shared::events::{
    ProcessEvent, FileEvent, NetworkEvent, EtwEvent
};
use crate::{
    comms::{
        memory_ring::{spawn_ring_consumer, MemoryRingBuses, MemoryRing},
        TokioBuses,
        WrappedEvent,
    },
    config::{load, Config},
    db::{
        connection::{init_database, db_path},
        maintenance::{spawn_ttl_cleanup, spawn_wal_maintenance},
        spawn_writer,
    }
};

#[macro_use]
extern crate metrics;
use metrics_exporter_prometheus::PrometheusBuilder;
use scanner::run_scanner;

const SERVICE_NAME: &str = "Gladix";

define_windows_service!(ffi_service_main, service_main);

/// One‑liner helper for fatal errors (bypasses the logger so it works early).
macro_rules! fatal {
    ($($arg:tt)+) => {{
        let msg = format!($($arg)+);
        log::error!("Fatal error occurred in {}. Exiting...", msg);
        std::process::exit(1);
    }};
}

/// Top‑level helper: directory where the running executable lives.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .expect("Cannot determine exe path")
        .parent()
        .expect("Executable must live in some directory")
        .to_path_buf()
}

/// Structured logging initialisation (stdout + optional file).
fn setup_logging(cfg: &Config, exe_dir: &Path) -> Result<(), fern::InitError> {
    let level = match cfg.logging.level.to_uppercase().as_str() {
        "ERROR" => LevelFilter::Error,
        "WARN"  => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        _       => LevelFilter::Info,
    };

    let log_path = cfg.logging.enable.then(|| {
        exe_dir.join(cfg.logging.file.as_deref().unwrap_or("agent.log"))
    });

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

/*──────────────────────────── helper functions ────────────────────────────*/

/// Create `Buses<E>` **and** spawn an async DB writer
/// (only usable for event types whose `WrappedEvent<E>` implements `BatchInsert`).
fn make_buses_with_writer<E>(
    rt: &Runtime,
    exe_dir: &Path,
    db_cfg: &config::model::DatabaseConfig,
) -> TokioBuses<E>
where
    E: Clone + Send + 'static,
    WrappedEvent<E>: db::batch_inserts::BatchInsert<WrappedEvent<E>>,
{
    let conn = init_database(exe_dir, db_cfg)
        .unwrap_or_else(|e| fatal!("[make_buses_with_writer()]: {}", e));

    // TODO: Maybe change to use new() instead?
    let (db_tx, db_rx) = async_mpsc::channel::<WrappedEvent<E>>(10_000);
    spawn_writer(rt, conn, db_rx, db_cfg);

    let (intel_tx, _) = broadcast::channel::<WrappedEvent<E>>(1_024);
    TokioBuses { db_tx, intel_tx }
}

/*──────────────────────────── main service routine ────────────────────────*/

fn run_service() {
    /* 1 ▸ Load configuration  */
    let exe_dir = exe_dir();
    let cfg = load(&exe_dir.join("config.toml"))
        .unwrap_or_else(|e| fatal!("run_service(): config: {}", e ));

    /* 2 ▸ Logging  */
    setup_logging(&cfg, &exe_dir).expect("Logging setup failed");
    log::info!("run_service(): Service bootstrap initiated");

    /* 3 ▸ Prometheus registry */
    let _prom = PrometheusBuilder::new().install();

    /* 4 ▸ Database schema migration (only once) */
    let db_cfg = &cfg.database;
    let db_path_on_disk = db_path(&exe_dir, db_cfg);
    init_database(&exe_dir, db_cfg)
        .unwrap_or_else(|e| fatal!("run_service(): database: {}", e));
    log::info!("run_service(): Database ready at {}", exe_dir.display());

    /* 5 ▸ Tokio runtime + async DB writers */
    let rt = Runtime::new().expect("Tokio runtime failed");

    let process = make_buses_with_writer::<ProcessEvent>(&rt, &exe_dir, db_cfg);
    let file    = make_buses_with_writer::<FileEvent>(&rt, &exe_dir, db_cfg);
    let net     = make_buses_with_writer::<NetworkEvent>(&rt, &exe_dir, db_cfg);
    let etw     = make_buses_with_writer::<EtwEvent>(&rt, &exe_dir, db_cfg,);
    log::info!("run_service(): TokioBuses ready");

    let memory_ring_buses = MemoryRingBuses {
        process: process.clone(),
        file   : file.clone(),
        net    : net.clone(),
        etw    : etw.clone(),
    };

    let ring = MemoryRing::open().unwrap_or_else(|e| fatal!("run_service(): Error opening the ring: {}", e));
    log::info!("run_service(): Ring ready");

    spawn_ring_consumer(&rt, ring, memory_ring_buses);
    log::info!("run_service(): Ring consumers ready");

    // SQLite maintenance tasks
    spawn_ttl_cleanup(&rt, db_path_on_disk.clone(), db_cfg);
    spawn_wal_maintenance(&rt, db_path_on_disk, db_cfg);
    log::info!("run_service(): SQLite maintenance tasks ready");

    /* 6 ▸ Windows SCM / console fallback */
    let (svc_tx, svc_rx) = mpsc::sync_channel(1);
    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |ctrl| match ctrl {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                log::warn!("run_service(): SCM stop/shutdown requested");
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


    /* 7 ▸ Directory scanner (blocking thread) */
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();

    let groups = cfg.scanner.clone();          // already validated
    let cache_path = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache_path));
    log::info!("run_service(): Directory Scanner ready");

    /* 8 ▸ Shutdown handshake */
    let _ = svc_rx.recv();                     // block until stop requested
    log::info!("run_service(): Shutdown requested...");
    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status).unwrap();
    log::info!("run_service(): Service stopped cleanly");
}

/*──────────────────────── Windows‑service glue ───────────────────────────*/

fn service_main(_args: Vec<OsString>) { run_service() }

fn main() {
    // When not launched by the SCM we fall back to console mode.
    if start(SERVICE_NAME, ffi_service_main).is_err() {
        eprintln!(
            "[{}][WARN ][main] Not running as service; starting in console mode.",
            Local::now().to_rfc3339()
        );
        run_service();
    }
}

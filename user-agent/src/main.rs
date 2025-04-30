// src/main.rs

//! Agent entry‑point: Windows service or console fallback.
//!
//! **Refactored** to leverage the new [`Config::load()`] API that returns a fully‑validated
//! runtime configuration.  All bespoke glue for reading TOML, converting risk groups, and
//! validating paths has been removed.
//!
//! Execution flow
//! ————————————————————————————————————————————————————————————————————————
//! 1. `Config::load()` merges embedded defaults → optional file → env vars → CLI.
//! 2. Structured logging initialised from `cfg.logging`.
//! 3. SQLite opened; async writers & maintenance tasks spawned.
//! 4. Windows SCM registration (or console fallback).
//! 5. Directory scanner launched in blocking thread.
//! 6. Graceful shutdown via service control or Ctrl‑C.

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
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{broadcast, mpsc as async_mpsc};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher::start,
};

use crate::comms::WrappedEvent;
use crate::config::{load, Config};
use shared::events::{ProcessEvent};
use db::{
    connection::{init_database, open_db_connection},
    maintenance::{spawn_ttl_cleanup, spawn_wal_maintenance},
    spawn_writer,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use scanner::run_scanner;
use crate::comms::listeners::{Buses, Listener, RingListener};
use crate::comms::memory_ring::MemoryRing;

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

/// Returns the directory that contains the running executable.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .expect("Cannot determine exe path")
        .parent()
        .expect("Executable must live in some directory")
        .to_path_buf()
}

/// Initialise structured logging according to `cfg.logging`.
fn setup_logging(cfg: &Config, exe_dir: &Path) -> Result<(), fern::InitError> {
    let level = match cfg.logging.level.to_uppercase().as_str() {
        "ERROR" => LevelFilter::Error,
        "WARN"  => LevelFilter::Warn,
        "DEBUG" => LevelFilter::Debug,
        "TRACE" => LevelFilter::Trace,
        _        => LevelFilter::Info,
    };

    let log_path = cfg
        .logging
        .enable
        .then(|| exe_dir.join(cfg.logging.file.as_deref().unwrap_or("agent.log")));

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

fn run_service() {
    // ────────────────────────────────────────────────────────────────────
    // 1 ▸ Context & configuration
    // ────────────────────────────────────────────────────────────────────
    let exe_dir = exe_dir();

    // Loader merges defaults → exe_dir/config.toml → env (APP__) → CLI (None here)
    let cfg = load(&exe_dir.join("config.toml"))
        .unwrap_or_else(|e| fatal!("config", "{}", e));

    // ────────────────────────────────────────────────────────────────────
    // 2 ▸ Logging
    // ────────────────────────────────────────────────────────────────────
    setup_logging(&cfg, &exe_dir).expect("Logging setup failed");
    log::info!("Service bootstrap initiated");

    // ────────────────────────────────────────────────────────────────────
    // 3 ▸ Prometheus metrics
    // ────────────────────────────────────────────────────────────────────
    let _recorder = PrometheusBuilder::new().install();

    // ────────────────────────────────────────────────────────────────────
    // 4 ▸ Database initialisation
    // ────────────────────────────────────────────────────────────────────
    let db_cfg  = &cfg.database;
    let db_path = db::connection::db_path(&exe_dir, db_cfg);

    let db_conn_process = init_database(&exe_dir, db_cfg).unwrap_or_else(|e| fatal!("database", "{e}"));

    // ────────────────────────────────────────────────────────────────────
    // 5 ▸ Tokio runtime & async DB writers
    // ────────────────────────────────────────────────────────────────────
    let rt = Runtime::new().expect("Tokio runtime failed");
    let (process_db_tx, process_db_rx) =
        async_mpsc::channel::<WrappedEvent<ProcessEvent>>(10_000);
    spawn_writer(&rt, db_conn_process, process_db_rx, db_cfg);

    // 5a ▸ MemoryRing listener para ProcessEvent
    let process_ring = MemoryRing::open(r"\\Gladix\process_ring")
        .unwrap_or_else(|e| fatal!("ring", "process_ring: {}", e));
    let process_listener = Arc::new(RingListener::<ProcessEvent>::new(
        "process",
        process_ring,
        "7119d098-3100-4fc2-ba48-52b1fabdb4b8",
    ));

    // Creamos también el canal de broadcast para intel
    let (process_intel_tx, _) =
        broadcast::channel::<WrappedEvent<ProcessEvent>>(1_024);

    // Ahora sí conectamos los _mismos_ senders al Buses…
    let process_buses = Buses {
        db_tx:    process_db_tx.clone(),
        intel_tx: process_intel_tx.clone(),
    };

    process_listener.spawn(process_buses);

    // Background DB‑maintenance tasks
    spawn_ttl_cleanup(&rt, db_path.clone(), db_cfg);
    spawn_wal_maintenance(&rt, db_path.clone(), db_cfg);

    // ────────────────────────────────────────────────────────────────────
    // 6 ▸ Windows SCM integration
    // ────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────
    // 7 ▸ Scanner thread
    // ────────────────────────────────────────────────────────────────────
    status.current_state = ServiceState::Running;
    status_handle.set_service_status(status.clone()).unwrap();

    let groups = cfg.scanner.clone(); // already runtime‑ready `RiskGroup`s
    log::info!("Service running with {} scanner groups", groups.len());

    let cache_path = exe_dir.join("persistent_cache.json");
    thread::spawn(move || run_scanner(groups, cache_path));

    // ────────────────────────────────────────────────────────────────────
    // 8 ▸ Shutdown
    // ────────────────────────────────────────────────────────────────────
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
    // When not launched by the SCM we fall back to console mode.
    if start(SERVICE_NAME, ffi_service_main).is_err() {
        eprintln!(
            "[{}][ERROR][main] Not a service; falling back to console.",
            Local::now().to_rfc3339()
        );
        run_service();
    }
}

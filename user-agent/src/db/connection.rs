// src/db/connection.rs
//! Opening and initialising SQLite with runtime parameters.

use std::{fs, path::{Path, PathBuf}, time::Duration};
use rusqlite::Connection;
use crate::config::types::DatabaseConfig;

pub fn db_path(exe_dir: &Path, cfg: &DatabaseConfig) -> PathBuf {
    exe_dir.join(&cfg.path)
}

pub fn open_db_connection(path: &Path, cfg: &DatabaseConfig) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(1_000))?;
    conn.pragma_update(None, "journal_mode", &"WAL")?;
    conn.pragma_update(None, "synchronous", &cfg.synchronous.as_str())?;
    Ok(conn)
}

pub fn init_database(exe_dir: &Path, cfg: &DatabaseConfig) -> rusqlite::Result<Connection> {
    let path = db_path(exe_dir, cfg);

    if cfg.purge_on_restart && path.exists() {
        let _ = fs::remove_file(&path);
    }
    let first_run = !path.exists();

    let conn = open_db_connection(&path, cfg)?;
    conn.pragma_update(None, "journal_size_limit", &(cfg.journal_size_limit as i64))?;

    if first_run {
        let schema = include_str!("../../resources/schema.sql");
        conn.execute_batch(schema)?;
    }
    log::info!("Database ready at {}", path.display());
    Ok(conn)
}

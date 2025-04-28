// src/db/maintenance.rs
//! Periodic TTL cleanup & WAL checkpoints.

use std::{path::PathBuf, time::Duration};
use rusqlite::Connection;
use tokio::runtime::Runtime;
use crate::config::types::DatabaseConfig;

pub fn spawn_ttl_cleanup(rt: &Runtime, db_path: PathBuf, cfg: &DatabaseConfig) {
    if cfg.ttl_seconds == 0 { return; }          // disabled
    let ttl = cfg.ttl_seconds as i64;
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60)); // every minute
        loop {
            ticker.tick().await;
            if let Ok(conn) = Connection::open(&db_path) {
                let cutoff = chrono::Utc::now().timestamp() - ttl;
                let _ = conn.execute("DELETE FROM fs_events       WHERE ts < ?1", [cutoff]);
                let _ = conn.execute("DELETE FROM network_events  WHERE ts < ?1", [cutoff]);
                let _ = conn.execute("DELETE FROM etw_events      WHERE ts < ?1", [cutoff]);
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                log::debug!("TTL cleanup removed events before {}", cutoff);
            }
        }
    });
}

pub fn spawn_wal_maintenance(rt: &Runtime, db_path: PathBuf, cfg: &DatabaseConfig) {
    let period = Duration::from_secs(cfg.checkpoint_seconds);
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(period);
        loop {
            ticker.tick().await;
            if let Ok(conn) = Connection::open(&db_path) {
                if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
                    log::warn!("WAL checkpoint failed: {}", e);
                }
            }
        }
    });
}

// src/db/mod.rs
//! Public façade for DB helpers (re-exports plus spawn_writer).

pub mod connection;
pub mod maintenance;
pub mod db_writer;
pub mod batch_inserts;

use rusqlite::Connection;
use tokio::{runtime::Runtime, sync::mpsc as async_mpsc};
use crate::config::types::DatabaseConfig;
pub use db_writer::DbWriter;
pub use batch_inserts::BatchInsert;


/// Spawn a dedicated writer task for events of type `E`.
pub fn spawn_writer<E>(
    rt: &Runtime,
    conn: Connection,
    rx: async_mpsc::Receiver<E>,
    cfg: &DatabaseConfig,
) where
    E: BatchInsert<E> + Send + 'static,
{
    // Copy what we need so nothing borrowed lives in the async task
    let flush_ms  = cfg.flush_interval_ms;
    let batch_sz  = cfg.batch_size;

    rt.spawn(async move {
        DbWriter::<E> {
            conn,
            rx,
            flush_interval_ms: flush_ms,
            batch_size:        batch_sz,
        }
            .run()
            .await;
    });
}


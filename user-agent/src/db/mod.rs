// src/db/mod.rs
//! Public façade for DB helpers (re-exports plus spawn_writer).

pub mod connection;
pub mod maintenance;
pub mod db_writer;
pub mod batch_inserts;

// src/db/mod.rs

use rusqlite::Connection;
use tokio::{runtime::Runtime, sync::mpsc as async_mpsc};

use crate::config::model::DatabaseConfig;
use crate::db::db_writer::DbWriter;
use crate::db::batch_inserts::BatchInsert;

/// Arranca un writer de SQLite para cualquier `T` que implemente:
///   - `BatchInsert<T>` (tiene el SQL y el bind_and_execute)
///   - `Send + Clone + 'static` (para poder moverse al task de Tokio)
pub fn spawn_writer<T>(
    rt: &Runtime,
    conn: Connection,
    rx: async_mpsc::Receiver<T>,
    cfg: &DatabaseConfig,
)
where
    T: BatchInsert<T> + Send + Clone + 'static,
{
    let flush_ms = cfg.flush_interval_ms;
    let batch_sz = cfg.batch_size;

    rt.spawn(async move {
        DbWriter::<T> {
            conn,
            rx,
            flush_interval_ms: flush_ms,
            batch_size:        batch_sz,
        }
            .run()
            .await;
    });
}
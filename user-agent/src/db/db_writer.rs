// src/db/db_writer.rs

use rusqlite::{Connection, Statement};
use std::{thread::sleep, time::{Duration, Instant}};
use thiserror::Error;

/// Defines how to insert a batch of events of type T
pub trait BatchInsert<T> {
    fn insert_sql() -> &'static str;
    fn bind_params(stmt: &mut Statement<'_>, record: &T) -> rusqlite::Result<()>;
}

/// A high-performance, batched writer for SQLite.
/// Performs all DB work synchronously to avoid holding &Connection across .await.
pub struct DbWriter<T> {
    pub conn: Connection,
    pub rx: tokio::sync::mpsc::Receiver<T>,
    pub flush_interval_ms: u64,
    pub batch_size: usize,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),
}

impl<T> DbWriter<T>
where
    T: Send + 'static,
{
    /// Start the writer loop; call inside tokio::spawn.
    pub async fn run(mut self)
    where
        T: BatchInsert<T>,
    {
        let mut buffer = Vec::with_capacity(self.batch_size);
        let mut interval = tokio::time::interval(Duration::from_millis(self.flush_interval_ms));

        loop {
            tokio::select! {
                maybe = self.rx.recv() => match maybe {
                    Some(ev) => {
                        buffer.push(ev);
                        if buffer.len() >= self.batch_size {
                            let _ = self.flush_sync(&mut buffer);
                        }
                    }
                    None => {
                        let _ = self.flush_sync(&mut buffer);
                        break;
                    }
                },
                _ = interval.tick() => {
                    let _ = self.flush_sync(&mut buffer);
                }
            }
        }
    }

    /// Synchronous flush with retry + backoff.
    fn flush_sync(&mut self, buffer: &mut Vec<T>) -> Result<(), DbError>
    where
        T: BatchInsert<T>,
    {
        let start = Instant::now();
        let mut attempts = 0;

        while !buffer.is_empty() {
            match self.conn.transaction() {
                Ok(tx) => {
                    {
                        let mut stmt = tx.prepare_cached(T::insert_sql())?;
                        for rec in buffer.drain(..) {
                            T::bind_params(&mut stmt, &rec)?;
                        }
                    }
                    tx.commit()?;

                    // TODO: record metrics here, e.g.
                    //   histogram!("db_flush_duration_seconds", start.elapsed().as_secs_f64());
                    //   histogram!("db_flush_batch_size", batch_size as f64);
                    //   counter!("db_flush_batches_total");
                }
                Err(e) if e.to_string().contains("database is locked") && attempts < 5 => {
                    attempts += 1;
                    sleep(Duration::from_millis(50 * attempts));
                }
                Err(e) => return Err(DbError::Sql(e)),
            }
        }
        Ok(())
    }
}

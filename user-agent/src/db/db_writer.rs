// src/db/db_writer.rs

use rusqlite::Connection;
use std::time::{Duration, Instant};
use thiserror::Error;
use metrics::{histogram, counter};
use crate::db::batch_inserts::BatchInsert; 

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
    T: Send + 'static + BatchInsert<T>,
{
    pub async fn run(mut self) {
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

    fn flush_sync(&mut self, buffer: &mut Vec<T>) -> Result<(), DbError> {
        let batch_count = buffer.len() as f64;
        if batch_count == 0.0 {
            return Ok(());
        }

        let start = Instant::now();
        let sql = T::insert_sql();
        let mut stmt = self.conn.prepare_cached(sql)?;

        for rec in buffer.drain(..) {
            T::bind_and_execute(&mut stmt, &rec)?;
        }

        // Record metrics
        let elapsed = start.elapsed().as_secs_f64();
        histogram!("db_flush_duration_seconds").record(elapsed);
        histogram!("db_flush_batch_size").record(batch_count);
        counter!("db_flush_batches_total").increment(1);

        Ok(())
    }
}



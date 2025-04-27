use rusqlite::{Connection, Statement};
use std::time::{Instant, Duration};
use std::thread::sleep;
use thiserror::Error;

/// Defines how to insert a batch of events of type T
pub trait BatchInsert<T> {
    fn insert_sql() -> &'static str;
    fn bind_params(stmt: &mut Statement<'_>, record: &T) -> rusqlite::Result<()>;
}

/// A high-performance, async-capable, batched writer for SQLite.
/// Runs on Tokio but performs DB work synchronously to avoid holding
/// SQLite connections across await points.
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
    /// Run the event loop - call inside tokio::spawn
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
                            if let Err(e) = self.flush_sync(&mut buffer) {
                                log::error!("Error flushing batch: {}", e);
                            }
                        }
                    }
                    None => {
                        if !buffer.is_empty() {
                            let _ = self.flush_sync(&mut buffer);
                        }
                        break;
                    }
                },
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        let _ = self.flush_sync(&mut buffer);
                    }
                }
            }
        }
    }

    /// Synchronous flush with retry + backoff
    fn flush_sync(&mut self, buffer: &mut Vec<T>) -> Result<(), DbError>
    where
        T: BatchInsert<T>,
    {
        let start = Instant::now();
        let mut attempts = 0;
        while !buffer.is_empty() {
            match self.conn.transaction() {
                Ok(transaction) => { 
                    let tx = transaction;
                    let sql = T::insert_sql(); 
                    // scope the statement so it’s dropped before commit
                    {
                       let mut stmt = tx.prepare_cached(sql)?;
                       for record in buffer.drain(..) {
                           T::bind_params(&mut stmt, &record)?;
                       } 
                    }
                    // now safe to move out of tx
                    tx.commit()?;
                    let elapsed = start.elapsed();
                    log::trace!("Flushed batch in {:?}", elapsed); 
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
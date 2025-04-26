// src/scheduler/worker.rs

//! # Worker
//!
//! Concurrently processes a batch of files: filters by size/extension,
//! computes their hash, and updates the shared cache.
//!
//! **Responsibilities:**
//! - Skip oversized or non‐executable files.
//! - Compute file hash and cache timestamp.
//! - Spawn a small fixed thread pool for parallelism.

use crate::gladix_log;
use log::Level;
use super::cache::{FileCache, FileCacheEntry};
use super::hash::{compute_file_hash, is_executable_file};
use std::{
    fs,
    io::ErrorKind,
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::UNIX_EPOCH,
};

/// Process a single `path`: skip if too big or non‐exe, else hash + cache.
fn process_file(
    path: &Path,
    cache: &FileCache,
    max_size: u64,
    exts: &[String],
) -> std::io::Result<()> {
    let meta = fs::metadata(path)?;
    if meta.len() > max_size || !is_executable_file(path, exts) {
        gladix_log!(
            Level::Debug,
            "worker",
            "Ignored {:?} (size={}, exe={})",
            path,
            meta.len(),
            is_executable_file(path, exts)
        );
        return Ok(());
    }

    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?
        .as_secs();

    let hash = compute_file_hash(path)?;
    let mut guard = cache.lock().unwrap();
    if let Some(e) = guard.get(path) {
        if e.timestamp == mtime && e.hash == hash {
            return Ok(()); // already up-to-date
        }
    }

    guard.insert(
        path.to_owned(),
        FileCacheEntry {
            hash,
            timestamp: mtime,
            scan_result: Some("Processed".into()),
        },
    );

    gladix_log!(Level::Debug, "worker", "Processed {:?} (hash={})", path, hash);
    Ok(())
}

/// Spawn a fixed‐size pool to handle `paths`, then wait for all to finish.
pub fn process_files(
    paths: Vec<PathBuf>,
    cache: FileCache,
    max_size: u64,
    exts: Arc<Vec<String>>,
) {
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let rx = Arc::new(Mutex::new(rx));

    let workers = (0..std::cmp::min(4, paths.len()))
        .map(|_| {
            let rx = Arc::clone(&rx);
            let cache = Arc::clone(&cache);
            let exts = Arc::clone(&exts);
            thread::spawn(move || {
                while let Ok(p) = rx.lock().unwrap().recv() {
                    let _ = process_file(&p, &cache, max_size, &exts);
                }
            })
        })
        .collect::<Vec<_>>();

    for p in paths {
        let _ = tx.send(p);
    }
    drop(tx);

    for h in workers {
        let _ = h.join();
    }
}

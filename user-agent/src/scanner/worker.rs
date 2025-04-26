// src/scanner/worker.rs

//! Concurrent file‐processing engine.

use crate::gladix_log;
use super::cache::FileCacheEntry;
use super::hash::{compute_file_hash, is_executable_file};
use std::{
    collections::HashMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::UNIX_EPOCH,
};

/// Checks file metadata and content hash to decide whether to process a file.
/// - Skips files larger than `max_size` or non-executable based on extension blacklist/whitelist.
/// - Uses timestamp + hash comparison to avoid reprocessing unchanged files.
fn process_file(
    path: &Path,
    cache: &Arc<Mutex<HashMap<PathBuf, FileCacheEntry>>>,
    max_size: u64,
    exts: &[String],
) -> std::io::Result<()> {
    let meta = fs::metadata(path)?;

    // Skip based on size or file type to minimize unnecessary I/O and hashing.
    if meta.len() > max_size || !is_executable_file(path, exts) {
        gladix_log!(Level::Debug, "Ignored {:?} (size={}, exe={})", path, meta.len(), is_executable_file(path, exts));
        return Ok(());
    }

    // Compute modification time in seconds since epoch for cache comparison.
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?
        .as_secs();

    // Hashing can be expensive; only do if size/type checks pass.
    let hash = compute_file_hash(path)?;

    // Lock cache to check prior processed entry (timestamp+hash match means skip).
    let mut lock = cache.lock().unwrap();
    if let Some(entry) = lock.get(path) {
        if entry.timestamp == mtime && entry.hash == hash {
            // File unchanged since last scan: skip further processing.
            return Ok(());
        }
    }

    // Record new cache entry with the scan result placeholder.
    lock.insert(
        path.to_owned(),
        FileCacheEntry { hash, timestamp: mtime, scan_result: Some("Processed".into()) },
    );
    gladix_log!(Level::Debug, "Processed {:?} (hash={})", path, hash);
    Ok(())
}

/// Distributes file paths to a pool of worker threads for concurrent processing.
/// - Uses up to 4 threads or number of files, whichever is smaller.
/// - Workers pull from a shared, synchronized receiver until channel closes.
pub fn process_files(
    paths: Vec<PathBuf>,
    cache: Arc<Mutex<HashMap<PathBuf, FileCacheEntry>>>,
    max_size: u64,
    exts: Arc<Vec<String>>,
) {
    // Channel for sending file paths to worker threads.
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let rx = Arc::new(Mutex::new(rx));  // Mutex ensures only one thread at a time reads

    // Spawn a bounded number of worker threads for parallel processing.
    let workers = (0..std::cmp::min(4, paths.len()))
        .map(|_| {
            let rx_clone = Arc::clone(&rx);
            let cache_clone = Arc::clone(&cache);
            let exts_clone = Arc::clone(&exts);
            thread::spawn(move || {
                // Each worker loops until channel is closed and empty.
                while let Ok(path) = rx_clone.lock().unwrap().recv() {
                    // Errors inside process_file are intentionally ignored here,
                    // but in a real implementation, consider logging or handling.
                    let _ = process_file(&path, &cache_clone, max_size, &exts_clone);
                }
            })
        })
        .collect::<Vec<_>>();

    // Feed all file paths into the channel, then close it to signal completion.
    for path in paths {
        let _ = tx.send(path);
    }
    drop(tx);  // Closing the sending side causes workers to exit when done.

    // Wait for all workers to finish before returning.
    for handle in workers {
        let _ = handle.join();
    }
}

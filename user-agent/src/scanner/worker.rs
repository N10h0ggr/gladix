//! Concurrent file processing engine.
//!
//! This module is responsible for processing individual files, checking
//! their eligibility (size and extension), computing hashes, and updating
//! the shared file scan cache. It runs processing jobs concurrently using
//! a fixed-size thread pool.
//!
//! Key responsibilities:
//!


use super::cache::{FileCache, FileCacheEntry};
use super::hash::{compute_file_hash, is_executable_file};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn process_file(path: &Path, cache: &FileCache, max_size: u64, exts: &[String]) -> std::io::Result<()> {
    let meta = fs::metadata(path)?;
    if meta.len() > max_size || !is_executable_file(path, exts) {
        return Ok(());
    }

    let mtime = meta.modified()?.duration_since(UNIX_EPOCH)?.as_secs();
    let hash = compute_file_hash(path)?;

    let mut lock = cache.lock().unwrap();
    if let Some(e) = lock.get(path) {
        if e.timestamp == mtime && e.hash == hash {
            return Ok(()); // ya en caché
        }
    }
    lock.insert(
        path.to_owned(),
        FileCacheEntry {
            hash,
            timestamp: mtime,
            scan_result: Some("Processed".into()),
        },
    );
    println!("Processed {} (hash {})", path.display(), hash);
    Ok(())
}


pub fn process_files(paths: Vec<PathBuf>, cache: FileCache, max_size: u64, exts: Arc<Vec<String>>) {
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

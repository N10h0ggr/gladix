// src/scanner/scheduler.rs

//! Task scheduler & directory scanner.

use super::cache::{load_persistent_cache, save_persistent_cache};
use super::worker::process_files;
use crate::config::model::RiskGroup;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Recursively list all files under a directory.
/// Logs count to aid debugging of deep directory trees.
fn list_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                // Recurse into subdirectories
                out.extend(list_files(&p));
            } else {
                out.push(p);
            }
        }
    }

    log::debug!( "list_files: {:?} → {} entries", dir, out.len());
    out
}

/// Launches one thread per risk group to perform scheduled scans.
/// Each thread:
/// 1. Logs start of scan pass.
/// 2. Lists files in each directory, skipping missing ones.
/// 3. Delegates to worker pool for concurrent file processing.
/// 4. Saves updated cache and sleeps until next interval.
pub fn run_scanner(groups: Vec<RiskGroup>, cache_path: PathBuf) {
    // Shared cache loaded once and passed to all threads
    let cache = Arc::new(Mutex::new(load_persistent_cache(&cache_path)));
    // Extensions to consider executable—immutable shared state
    let exts = Arc::new(vec!["exe".into(), "dll".into(), "sys".into(), "ocx".into()]);
    // Maximum file size to process (50 MB)
    let max_size = 50 * 1024 * 1024;

    log::info!( "Scheduling {} group(s)", groups.len());

    for group in groups {
        let cache_cloned = Arc::clone(&cache);
        let exts_cloned = Arc::clone(&exts);
        let cache_file = cache_path.clone();
        // Capture directories and scan interval ahead of thread loop
        let dirs: Vec<PathBuf> = group.directories.into_iter().collect();
        let secs = group
            .interval
            .expect("scheduled_interval must be set")
            .as_secs();

        thread::spawn(move || {
            log::info!( "Thread for {:?} starting (interval={}s)", group.risk, secs);

            loop {
                log::info!( "[{:?}] Starting scan pass", group.risk);

                for dir in &dirs {
                    if !dir.exists() {
                        // Warn and skip directories that may have been removed
                        log::warn!("Skipping non-existent dir: {:?}", dir);
                        continue;
                    }
                    log::info!("Scanning {:?}", dir);

                    // Collect candidate files (expensive I/O)
                    let files = list_files(dir);
                    log::debug!( "Found {} candidates in {:?}", files.len(), dir);

                    // Parallel processing; ignores errors inside
                    process_files(files, Arc::clone(&cache_cloned), max_size, Arc::clone(&exts_cloned));
                }

                // Persist updated cache after each pass
                save_persistent_cache(&cache_file, &*cache_cloned.lock().unwrap());
                log::info!( "[{:?}] Cache written to {:?}", group.risk, cache_file);
                log::debug!( "[{:?}] Sleeping for {}s", group.risk, secs);

                // Sleep until next scheduled scan iteration
                thread::sleep(Duration::from_secs(secs));
            }
        });
    }

    // Main thread idles to keep process alive; caches are saved by worker threads
    loop {
        thread::sleep(Duration::from_secs(600));
    }
}

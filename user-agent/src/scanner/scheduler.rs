// src/scheduler/scheduler.rs

//! # Scheduler
//!
//! Periodically scans directories by risk group, spawns workers, and
//! persists results to cache.
//!
//! **Responsibilities:**
//! - Recursively list files in each risk‐group directory.
//! - Spawn per‐group threads that loop forever.
//! - Delegate file processing to `worker`.

use crate::gladix_log;
use log::Level;
use super::cache::{load_persistent_cache, save_persistent_cache};
use super::worker::process_files;
use crate::config::types::RiskGroup;
use std::{fs, path::PathBuf, sync::{Arc, Mutex}, thread, time::Duration};

/// Recursively list **all** files under `dir`.
fn list_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(list_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    gladix_log!(Level::Debug, "list_files: {:?} → {} entries", dir, out.len());
    out
}

/// Spawn one thread *per* `groups`, run an immediate scan, then sleep forever.
pub fn run_scanner(groups: Vec<RiskGroup>, cache_path: PathBuf) {
    let cache = Arc::new(Mutex::new(load_persistent_cache(&cache_path)));
    let exts = Arc::new(vec!["exe".into(), "dll".into(), "sys".into(), "ocx".into()]);
    let max_size = 50 * 1024 * 1024;

    gladix_log!(Level::Info, "run_scanner: scheduling {} group(s)", groups.len());

    for group in groups {
        let cache = Arc::clone(&cache);
        let exts = Arc::clone(&exts);
        let cache_file = cache_path.clone();
        let dirs: Vec<PathBuf> = group
            .directories
            .into_iter()
            .filter(|d| !d.as_os_str().is_empty())
            .map(PathBuf::from)
            .collect();
        let secs = group
            .scheduled_interval
            .expect("scheduled_interval must be set")
            .as_secs();

        thread::spawn(move || {
            gladix_log!(
                Level::Info,
                "scanner",
                "Thread for {:?} starting (interval={}s)",
                group.risk,
                secs
            );

            loop {
                gladix_log!(
                    Level::Info,
                    "scanner",
                    "[{:?}] Starting scan pass",
                    group.risk
                );

                for dir in &dirs {
                    if !dir.exists() {
                        gladix_log!(
                            Level::Warn,
                            "scanner",
                            "Skipping non-existent dir: {:?}",
                            dir
                        );
                        continue;
                    }
                    gladix_log!(Level::Info, "scanner", "Scanning {:?}", dir);
                    let files = list_files(dir);
                    gladix_log!(
                        Level::Debug,
                        "scanner",
                        "Found {} candidates in {:?}",
                        files.len(),
                        dir
                    );
                    process_files(files, Arc::clone(&cache), max_size, Arc::clone(&exts));
                }

                save_persistent_cache(&cache_file, &*cache.lock().unwrap());
                gladix_log!(
                    Level::Info,
                    "scanner",
                    "[{:?}] Cache written to {:?}",
                    group.risk,
                    cache_file
                );

                gladix_log!(
                    Level::Debug,
                    "scanner",
                    "[{:?}] Sleeping for {}s",
                    group.risk,
                    secs
                );
                thread::sleep(Duration::from_secs(secs));
            }
        });
    }

    // Keep main thread alive
    loop {
        thread::sleep(Duration::from_secs(600));
    }
}

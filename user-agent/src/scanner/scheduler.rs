//! Explora directorios y planifica escaneos.
//! Task scheduler and directory scanner for the static analysis module.
//!
//! This module is responsible for managing the execution of scans across
//! configured directories, either on a scheduled basis or triggered manually.
//! It performs recursive directory traversal and delegates file-level analysis
//! to the worker module.
//!
//! Key responsibilities:
//! - Periodically scan directories based on risk group intervals.
//! - Traverse directories recursively and collect file paths.
//! - Invoke concurrent file processing for collected paths.
//! - Manage persistent cache updates after each scan cycle.
//!
//! This module is invoked from the main scanner entry point and runs
//! in a loop to handle continuous monitoring.

use super::cache::{load_persistent_cache, save_persistent_cache};
use super::worker::{process_files};
use crate::config::types::RiskGroup;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration};


/// Devuelve todos los ficheros de un directorio (de forma recursiva).
fn list_files(dir: &Path) -> Vec<PathBuf> {
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
    out
}


/// Spawn one thread per group, run an immediate scan, then sleep.
/// Spawns one thread per risk group. Each thread runs a scan immediately,
/// then sleeps for `interval` seconds before repeating.
pub fn run_scanner(groups: Vec<RiskGroup>, cache_path: PathBuf) {
    // Load or initialize the on-disk cache
    let cache = Arc::new(Mutex::new(load_persistent_cache(&cache_path)));

    // Allowed executable extensions
    let exts = Arc::new(vec!["exe".into(), "dll".into(), "sys".into(), "ocx".into()]);

    // Maximum file size: 50 MB
    let max_size = 50 * 1024 * 1024;

    println!("run_scanner(): scheduling {} group(s)", groups.len());

    for group in groups {
        let cache = Arc::clone(&cache);
        let exts = Arc::clone(&exts);
        let cache_file = cache_path.clone();

        // Convert and filter out empty paths
        let dirs: Vec<PathBuf> = group
            .directories
            .into_iter()
            .filter(|d| !d.as_os_str().is_empty())
            .map(PathBuf::from)
            .collect();

        // Interval to wait between scans
        let interval: Duration = group.scheduled_interval
            .expect("scheduled_interval must be set for each group");
        let secs = interval.as_secs();

        // Spawn a dedicated thread
        thread::spawn(move || {
            println!("→ Scanner thread for {:?} starting (interval = {}s)", group.risk, secs);

            loop {
                println!("  [{:?}] Starting scan pass", group.risk);

                for dir in &dirs {
                    if !dir.exists() {
                        println!("    [skip] directory does not exist: {:?}", dir);
                        continue;
                    }
                    println!("    scanning {:?}", dir);

                    let files = list_files(dir);
                    println!("    found {} candidates", files.len());

                    process_files(files, Arc::clone(&cache), max_size, Arc::clone(&exts));
                }

                // Persist cache each pass
                save_persistent_cache(&cache_file, &*cache.lock().unwrap());
                println!("  [{:?}] Cache written to {:?}", group.risk, cache_file);

                println!("  [{:?}] Sleeping for {}s\n", group.risk, secs);
                thread::sleep(interval);
            }
        });
    }

    // Keep main thread alive
    loop {
        thread::sleep(Duration::from_secs(600));
    }
}

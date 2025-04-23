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

/// Punto único de entrada que expone el módulo.
pub fn run_scanner(groups: Vec<RiskGroup>, cache_path: PathBuf) {
    let cache = Arc::new(Mutex::new(load_persistent_cache(&cache_path)));
    let exts = Arc::new(vec!["exe".into(), "dll".into(), "sys".into(), "ocx".into()]);
    let max_size = 50 * 1024 * 1024; // 50 MB

    for g in groups {
        let cache = Arc::clone(&cache);
        let exts = Arc::clone(&exts);
        let cache_file = cache_path.clone();

        thread::spawn(move || {
            let interval = g.scheduled_interval.unwrap();
            loop {
                for dir in &g.directories {
                    let files = list_files(dir);
                    process_files(files, Arc::clone(&cache), max_size, Arc::clone(&exts));
                }
                save_persistent_cache(&cache_file, &*cache.lock().unwrap());
                thread::sleep(interval);
            }
        });
    }

    // bloqueamos el hilo principal
    loop {
        thread::sleep(Duration::from_secs(600));
    }
}

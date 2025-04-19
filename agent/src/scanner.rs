use crate::cache::{load_persistent_cache, save_persistent_cache, FileCacheEntry};
use crate::config::RiskGroup;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use twox_hash::XxHash64;

/// Shared type for the file cache.
pub type FileCache = Arc<Mutex<HashMap<PathBuf, FileCacheEntry>>>;

/// Maximum allowed file size for scanning (e.g., 50 MB).
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Recursively scans a directory and returns all file paths.
fn scan_directory(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(scan_directory(&path));
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

/// Computes the hash of a file using XxHash64.
fn compute_file_hash(path: &Path) -> Result<u64, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let seed: u64 = 0;
    Ok(XxHash64::oneshot(seed, &buffer))
}

/// Checks if the file is considered executable based on its extension.
/// Only files with extensions "exe", "dll", "sys", or "ocx" will be processed.
fn is_executable_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_ascii_lowercase();
        return ext == "exe" || ext == "dll" || ext == "sys" || ext == "ocx";
    }
    false
}

/// Processes an individual file:
///   - Skips files that exceed MAX_FILE_SIZE or are not executable (with no log output for these).
///   - Retrieves the file's last modified time (as seconds since UNIX_EPOCH).
///   - Computes the file hash and updates the shared cache if the file is new or modified.
/// If the file is processed, a log message is printed.
fn process_file(file_path: &Path, cache: &FileCache) -> Result<(), std::io::Error> {
    let metadata = fs::metadata(file_path)?;

    // Skip large files.
    if metadata.len() > MAX_FILE_SIZE {
        return Ok(());
    }

    // Skip non-executable files.
    if !is_executable_file(file_path) {
        return Ok(());
    }

    let modified_time = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let file_hash = compute_file_hash(file_path)?;

    let mut cache_lock = cache.lock().unwrap();
    if let Some(entry) = cache_lock.get(file_path) {
        if entry.timestamp == modified_time && entry.hash == file_hash {
            return Ok(());
        }
    }
    let new_entry = FileCacheEntry {
        hash: file_hash,
        timestamp: modified_time,
        scan_result: Some("Processed".to_string()),
    };
    cache_lock.insert(file_path.to_owned(), new_entry);
    // Log only analyzed (processed) files.
    println!("Processed {} (hash: {})", file_path.display(), file_hash);
    Ok(())
}

/// Processes a list of files concurrently using a worker thread pool.
fn process_files_concurrently(file_paths: Vec<PathBuf>, cache: FileCache) {
    let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
    let rx = Arc::new(Mutex::new(rx));
    let num_workers = std::cmp::min(4, file_paths.len());
    let mut handles = Vec::with_capacity(num_workers);

    for _ in 0..num_workers {
        let rx = Arc::clone(&rx);
        let cache = Arc::clone(&cache);
        let handle = thread::spawn(move || loop {
            let file_path = {
                let lock = rx.lock().unwrap();
                lock.recv()
            };
            match file_path {
                Ok(path) => {
                    if let Err(e) = process_file(&path, &cache) {
                        eprintln!("Error processing {}: {:?}", path.display(), e);
                    }
                }
                Err(_) => break,
            }
        });
        handles.push(handle);
    }

    for path in file_paths {
        if let Err(e) = tx.send(path) {
            eprintln!("Error sending file path: {:?}", e);
        }
    }
    drop(tx);
    for handle in handles {
        let _ = handle.join();
    }
}

/// Scans a directory and processes its files. When all files have been processed,
/// prints a message indicating that analysis for this directory has finished.
fn scan_and_process_directory(dir: &Path, cache: &FileCache) {
    let files = scan_directory(dir);
    // Note: We no longer log the count of total files from the directory.
    process_files_concurrently(files, Arc::clone(cache));
    println!("Finished analyzing directory: {}", dir.display());
}

/// Runs a scheduled scan for the given risk group. For each scheduled group, all directories
/// in the group are scanned at the specified interval and the persistent cache is updated.
fn scheduled_risk_group_scan(group: RiskGroup, cache: FileCache, persist_path: PathBuf) {
    // We assume that scheduled_interval is defined (Some).
    let interval = group.scheduled_interval.unwrap();
    loop {
        println!(
            "Starting scheduled scan for risk {:?} ({} directories)",
            group.risk,
            group.directories.len()
        );
        for dir in &group.directories {
            scan_and_process_directory(dir, &cache);
        }
        {
            let cache_lock = cache.lock().unwrap();
            save_persistent_cache(&persist_path, &*cache_lock);
        }
        thread::sleep(interval);
    }
}

/// For manual risk groups (with no scheduled_interval), this function triggers a scan.
pub fn trigger_manual_scan(group: RiskGroup, cache: FileCache, persist_path: PathBuf) {
    for dir in &group.directories {
        scan_and_process_directory(dir, &cache);
    }
    {
        let cache_lock = cache.lock().unwrap();
        save_persistent_cache(&persist_path, &*cache_lock);
    }
}

/// Entry point for the scanner module. Loads the persistent cache and spawns a scheduled scan
/// for each risk group that is configured with a scheduled_interval.
pub fn run_scanner(risk_groups: Vec<crate::config::RiskGroup>, persist_cache_file: PathBuf) {
    let loaded_cache = crate::cache::load_persistent_cache(&persist_cache_file);
    let cache: FileCache = Arc::new(Mutex::new(loaded_cache));

    for group in risk_groups.into_iter() {
        if group.scheduled_interval.is_some() {
            let cache_clone = Arc::clone(&cache);
            let persist_clone = persist_cache_file.clone();
            thread::spawn(move || {
                scheduled_risk_group_scan(group, cache_clone, persist_clone);
            });
        }
    }

    // Block indefinitely; in a real agent you might implement graceful shutdown.
    loop {
        thread::sleep(Duration::from_secs(600));
    }
}

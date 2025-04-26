// src/scheduler/hash.rs

//! # Hashing Utilities
//!
//! Fast file hashing and extension‐based filtering.
//!
//! **Responsibilities:**
//! - Compute `XxHash64` of file contents.
//! - Detect executable files by extension.

use crate::gladix_log;
use log::Level;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use twox_hash::XxHash64;

/// Returns `true` if `path` has an extension in `exts`.
pub fn is_executable_file(path: &Path, exts: &[String]) -> bool {
    let result = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .map_or(false, |ext| {
            exts.iter().any(|allowed| allowed.eq_ignore_ascii_case(&ext))
        });
    gladix_log!(Level::Debug, "is_executable_file: {:?} → {}", path, result);
    result
}

/// Compute and return the `XxHash64` of a file’s contents.
pub fn compute_file_hash(path: &Path) -> std::io::Result<u64> {
    let f = File::open(path)?;
    let mut buf = Vec::new();
    let mut rdr = BufReader::new(f);
    rdr.read_to_end(&mut buf)?;
    let hash = XxHash64::oneshot(0, &buf);
    gladix_log!(Level::Debug, "compute_file_hash: {:?} → {}", path, hash);
    Ok(hash)
}

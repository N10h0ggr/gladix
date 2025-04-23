//! File hashing and type filtering utilities.
//!
//! This module provides utility functions used during static file analysis,
//! including file content hashing and executable type detection based on
//! file extension. It helps reduce workload by filtering non-executable
//! or oversized files before deeper inspection.
//!
//! Key responsibilities:
//! - Compute file hashes using XxHash64 (fast non-cryptographic hash).
//! - Identify files with executable extensions.
//! - Perform lightweight pre-analysis checks.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use twox_hash::XxHash64;

pub fn is_executable_file(path: &Path, exts: &[String]) -> bool {
    match path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()) {
        Some(ext) => exts.iter().any(|e| e.eq_ignore_ascii_case(&ext)),
        None => false,
    }
}

pub fn compute_file_hash(path: &Path) -> std::io::Result<u64> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(XxHash64::oneshot(0, &buf))
}

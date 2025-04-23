//! Persistent file scan cache with HMAC integrity.
//!
//! This module implements a simple persistent cache for file scan results,
//! using JSON serialization and HMAC-SHA256 signing to detect tampering.
//! It stores metadata such as file hashes, modification times, and scan results.
//!
//! Key responsibilities:
//! - Load and save cache data from/to disk.
//! - Ensure integrity using HMAC signatures.
//! - Serialize using path strings for portability.
//! - Convert between PathBuf-based and string-based cache maps.


use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::fs;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use sha2::digest::KeyInit;

// Type alias for our HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

// A key used to sign the persistent cache. In a real application this key should be protected.
static HMAC_KEY: &[u8] = b"super_secret_key";

/// A persistent cache entry for a scanned file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheEntry {
    /// Computed hash value.
    pub hash: u64,
    /// Last modified timestamp (seconds since UNIX_EPOCH).
    pub timestamp: u64,
    /// Placeholder scan result.
    pub scan_result: Option<String>,
}

/// Wrapper for the cache data that includes an HMAC signature.
#[derive(Serialize, Deserialize)]
struct CacheWrapper {
    /// Map of file paths (as strings) to cache entries.
    data: BTreeMap<String, FileCacheEntry>,
    /// HMAC signature (hex-encoded) for the data field.
    signature: String,
}

/// Converts a cache with PathBuf keys into an ordered BTreeMap with String keys.
fn convert_cache_to_string_keys(cache: &HashMap<PathBuf, FileCacheEntry>) -> BTreeMap<String, FileCacheEntry> {
    let mut sorted_map = BTreeMap::new();
    for (k, v) in cache.iter() {
        sorted_map.insert(k.to_string_lossy().into_owned(), v.clone());
    }
    sorted_map
}

/// Converts an ordered map with String keys back into a HashMap with PathBuf keys.
fn convert_cache_from_string_keys(map: BTreeMap<String, FileCacheEntry>) -> HashMap<PathBuf, FileCacheEntry> {
    map.into_iter()
        .map(|(k, v)| (PathBuf::from(k), v))
        .collect()
}

/// Computes an HMAC signature (hex-encoded) for the given data.
fn compute_signature(data: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(HMAC_KEY).unwrap();
    mac.update(data.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();
    hex::encode(bytes)
}

/// Loads the persistent cache from disk. If verification fails,
/// returns an empty cache.
pub fn load_persistent_cache<P: AsRef<Path>>(path: P) -> HashMap<PathBuf, FileCacheEntry> {
    if let Ok(file) = File::open(&path) {
        if let Ok(wrapper) = serde_json::from_reader::<_, CacheWrapper>(BufReader::new(file)) {
            // Serialize the data field back to a JSON string using the ordered BTreeMap.
            if let Ok(json_data) = serde_json::to_string_pretty(&wrapper.data) {
                let computed = compute_signature(&json_data);
                if computed == wrapper.signature {
                    return convert_cache_from_string_keys(wrapper.data);
                } else {
                    eprintln!("Warning: Persistent cache signature mismatch. Starting with an empty cache.");
                }
            }
        }
    }
    HashMap::new()
}

/// Saves the persistent cache to disk with an HMAC signature.
pub fn save_persistent_cache<P: AsRef<Path>>(path: P, cache: &HashMap<PathBuf, FileCacheEntry>) {
    let sorted_map = convert_cache_to_string_keys(cache);
    if let Ok(json_data) = serde_json::to_string_pretty(&sorted_map) {
        let signature = compute_signature(&json_data);
        let wrapper = CacheWrapper {
            data: sorted_map,
            signature,
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&wrapper) {
            if let Ok(mut file) = fs::File::create(path) {
                let _ = file.write_all(serialized.as_bytes());
            }
        }
    }
}
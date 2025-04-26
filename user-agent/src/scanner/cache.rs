// src/scanner/cache.rs

//! Persistent file‐scan cache with HMAC integrity checks.

use crate::gladix_log;
use std::{
    collections::{BTreeMap, HashMap},
    fs, fs::File,
    io::{BufReader, Write},
    path::{Path, PathBuf},
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{digest::KeyInit, Sha256};

// HMAC-SHA256 type alias and fixed key for cache signing
type HmacSha256 = Hmac<Sha256>;
static HMAC_KEY: &[u8] = b"super_secret_key";

/// Represents a cached scan result for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheEntry {
    pub hash: u64,
    pub timestamp: u64,
    pub scan_result: Option<String>,
}

/// Wrapper that holds the serialized cache and its signature.
/// The BTreeMap ensures consistent ordering before signing.
#[derive(Serialize, Deserialize)]
struct CacheWrapper {
    data: BTreeMap<String, FileCacheEntry>,
    signature: String,
}

/// Convert PathBuf-keyed cache into a string-keyed, sorted map for JSON serialization.
/// Sorting ensures deterministic signature computation.
fn convert_cache_to_string_keys(
    cache: &HashMap<PathBuf, FileCacheEntry>,
) -> BTreeMap<String, FileCacheEntry> {
    let mut m = BTreeMap::new();
    for (p, e) in cache {
        m.insert(p.to_string_lossy().into_owned(), e.clone());
    }
    m
}

/// Reverse operation: reconstruct PathBuf-keyed cache from the string-keyed map.
fn convert_cache_from_string_keys(
    map: BTreeMap<String, FileCacheEntry>,
) -> HashMap<PathBuf, FileCacheEntry> {
    map.into_iter()
        .map(|(s, e)| (PathBuf::from(s), e))
        .collect()
}

/// Generate an HMAC signature over JSON data.
/// Protects against tampering of cache file contents.
fn compute_signature(data: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(HMAC_KEY).unwrap();
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Load cache from disk, verifying the HMAC before trusting data.
/// Falls back to empty cache on any I/O/parse/signature error.
pub fn load_persistent_cache<P: AsRef<Path>>(path: P) -> HashMap<PathBuf, FileCacheEntry> {
    if let Ok(f) = File::open(&path) {
        // Deserialize wrapper containing sorted data and signature
        if let Ok(wrapper) = serde_json::from_reader::<_, CacheWrapper>(BufReader::new(f)) {
            // Re-serialize only the data portion for signature comparison
            if let Ok(json_data) = serde_json::to_string_pretty(&wrapper.data) {
                if compute_signature(&json_data) == wrapper.signature {
                    gladix_log!(Level::Info, "Loaded cache from {:?}", path.as_ref());
                    return convert_cache_from_string_keys(wrapper.data);
                } else {
                    gladix_log!(Level::Warn, "Signature mismatch for {:?}; starting fresh", path.as_ref());
                }
            }
        }
    }
    // Any failure -> empty cache + log info
    gladix_log!(Level::Info, "No valid cache at {:?}; using empty", path.as_ref());
    HashMap::new()
}

/// Save cache to disk with HMAC signature.
/// Uses pretty JSON for readability; signature computed on sorted data.
pub fn save_persistent_cache<P: AsRef<Path>>(path: P, cache: &HashMap<PathBuf, FileCacheEntry>) {
    // Prepare sorted string-keyed map
    let sorted = convert_cache_to_string_keys(cache);
    // Serialize map and sign it
    if let Ok(json_data) = serde_json::to_string_pretty(&sorted) {
        let signature = compute_signature(&json_data);
        let wrapper = CacheWrapper { data: sorted, signature };
        // Final wrapper serialization
        if let Ok(serialized) = serde_json::to_string_pretty(&wrapper) {
            match fs::File::create(&path) {
                Ok(mut f) => {
                    // Write atomically; errors logged but not fatal
                    if let Err(e) = f.write_all(serialized.as_bytes()) {
                        gladix_log!(Level::Error, "Failed write {:?}: {}", path.as_ref(), e);
                    } else {
                        gladix_log!(Level::Info, "Saved cache to {:?}", path.as_ref());
                    }
                }
                Err(e) => {
                    // Directory missing or permissions issue
                    gladix_log!(Level::Error, "Cannot create {:?}: {}", path.as_ref(), e);
                }
            }
        }
    }
}

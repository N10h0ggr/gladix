// src/scheduler/cache.rs

//! # Cache Module
//!
//! Provides a persistent file‐scan cache with HMAC‐SHA256 integrity checks.
//!
//! **Responsibilities:**
//! - Load cache from disk, verify HMAC signature.
//! - Save cache to disk with updated signature.
//! - Convert between `PathBuf` keys and ordered string keys.

use crate::gladix_log;
use log::Level;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{digest::KeyInit, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Shared in‐memory file cache for worker threads.
pub type FileCache = Arc<Mutex<HashMap<PathBuf, FileCacheEntry>>>;

type HmacSha256 = Hmac<Sha256>;
static HMAC_KEY: &[u8] = b"super_secret_key";

/// A cache entry for a scanned file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheEntry {
    pub hash: u64,
    pub timestamp: u64,
    pub scan_result: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CacheWrapper {
    data: BTreeMap<String, FileCacheEntry>,
    signature: String,
}

fn convert_cache_to_string_keys(
    cache: &HashMap<PathBuf, FileCacheEntry>,
) -> BTreeMap<String, FileCacheEntry> {
    let mut sorted = BTreeMap::new();
    for (p, entry) in cache {
        sorted.insert(p.to_string_lossy().into_owned(), entry.clone());
    }
    sorted
}

fn convert_cache_from_string_keys(
    map: BTreeMap<String, FileCacheEntry>,
) -> HashMap<PathBuf, FileCacheEntry> {
    map.into_iter()
        .map(|(s, e)| (PathBuf::from(s), e))
        .collect()
}

fn compute_signature(data: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(HMAC_KEY).unwrap();
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Load the cache from `path`. On any error or HMAC‐mismatch, returns an empty cache.
pub fn load_persistent_cache<P: AsRef<Path>>(path: P) -> HashMap<PathBuf, FileCacheEntry> {
    if let Ok(file) = File::open(&path) {
        if let Ok(wrapper) = serde_json::from_reader::<_, CacheWrapper>(BufReader::new(file)) {
            if let Ok(json) = serde_json::to_string_pretty(&wrapper.data) {
                let sig = compute_signature(&json);
                if sig == wrapper.signature {
                    gladix_log!(Level::Info, "cache", "Loaded cache from {:?}", path.as_ref());
                    return convert_cache_from_string_keys(wrapper.data);
                } else {
                    gladix_log!(
                        Level::Warn,
                        "cache",
                        "Signature mismatch for {:?}; starting fresh",
                        path.as_ref()
                    );
                }
            }
        }
    }
    gladix_log!(Level::Info, "cache", "No valid cache at {:?}; using empty", path.as_ref());
    HashMap::new()
}

/// Save the `cache` to `path`, signing it with HMAC.
pub fn save_persistent_cache<P: AsRef<Path>>(path: P, cache: &HashMap<PathBuf, FileCacheEntry>) {
    let sorted = convert_cache_to_string_keys(cache);
    if let Ok(json_data) = serde_json::to_string_pretty(&sorted) {
        let sig = compute_signature(&json_data);
        let wrapper = CacheWrapper { data: sorted, signature: sig };
        if let Ok(serialized) = serde_json::to_string_pretty(&wrapper) {
            match fs::File::create(&path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(serialized.as_bytes()) {
                        gladix_log!(Level::Error, "cache", "Failed write {:?}: {}", path.as_ref(), e);
                    } else {
                        gladix_log!(Level::Info, "cache", "Saved cache to {:?}", path.as_ref());
                    }
                }
                Err(e) => {
                    gladix_log!(Level::Error, "cache", "Cannot create {:?}: {}", path.as_ref(), e);
                }
            }
        }
    }
}

//! Integration tests for the static scanner module.
//!
//! This test suite verifies the behavior of file scanning under
//! different directory configurations and file system conditions.
//! It ensures that only eligible files are scanned, hashes are correct,
//! and the persistent cache is updated appropriately.
//!
//! Key responsibilities:
//! - Test executable filtering logic and size limits.
//! - Check hash computation and cache hits/misses.
//! - Simulate concurrent directory scans and validate cache integrity.
//! - Confirm scheduled scan intervals are honored.

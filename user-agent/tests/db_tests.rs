//! Integration tests for the SQLite WAL database layer.
//!
//! This test module validates the behavior of the database abstraction,
//! ensuring correct insertion, purging, and basic integrity constraints
//! for telemetry data.
//!
//! Key responsibilities:
//! - Verify batch inserts and WAL behavior.
//! - Validate purging logic by age or size.
//! - Check consistency across simulated restarts.
//! - Detect SQLite-specific edge cases (locks, cache misses).

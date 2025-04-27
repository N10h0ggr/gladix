//! SQLite WAL-based storage layer for telemetry events.
//!
//! This module provides a lightweight wrapper over a SQLite database,
//! optimized for write-heavy usage via Write-Ahead Logging (WAL).
//!
//! Key responsibilities:
//! - Insert telemetry events into the database.
//! - Perform age-based cleanup and purging.
//! - Support batch inserts for performance.
//! - Apply SQLite tuning via pragmas.

pub mod db_writer;
pub mod batch_inserts;
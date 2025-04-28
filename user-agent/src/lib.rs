// src/lib.rs
// ────────────────────────────────────────────────────────────────────────────
// Public library entry point.  Re-export everything for both `main.rs` and
// integration tests.

pub mod config;
pub mod db;
pub mod comms;
pub mod scanner;
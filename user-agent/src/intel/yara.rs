//! YARA rules engine with hot-reload support.
//!
//! This module manages the lifecycle of YARA rules used in static analysis.
//! It supports on-the-fly reloading when rules are updated on disk, avoiding
//! the need to restart the service.
//!
//! Key responsibilities:
//! - Load and compile YARA rules from a local directory.
//! - Monitor file system changes to support hot-reload.
//! - Match rules against file buffers or metadata.
//! - Provide integration with the static scanner.

//! File I/O minifilter subsystem for file activity monitoring.
//!
//! This module acts as the root for all file-related minifilter operations.
//! It provides an interface to register the minifilter, intercept file
//! operations (e.g., create, write), and relay relevant events to user space.
//!
//! Key responsibilities:
//! - Register the minifilter driver at a high altitude.
//! - Attach to file system volumes.
//! - Forward selected I/O events to user space via `FltSendMessage`.
//! - Serve as an integration point for file-specific filters (pre-create, etc.).

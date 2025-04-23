//! ETW consumer for real-time system event monitoring.
//!
//! This module sets up and runs an Event Tracing for Windows (ETW) session,
//! subscribing to selected system providers to receive process, network,
//! and image load events.
//!
//! Key responsibilities:
//! - Subscribe to ETW providers of interest.
//! - Parse and dispatch received events to the detection engine.
//! - Run within a shared thread pool for efficiency.
//! - Handle buffer overruns and session restarts if needed.

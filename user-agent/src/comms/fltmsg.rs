//! Kernel-to-user communication using Filter Manager messaging.
//!
//! This module implements the receiver side of `FltSendMessage`, allowing
//! the user-agent to accept and process telemetry messages sent by the
//! kernel-mode file minifilter.
//!
//! Key responsibilities:
//! - Connect to the filter port established by the kernel driver.
//! - Listen for messages in a background thread.
//! - Deserialize and forward file events to the detection engine.
//! - Handle disconnects and reconnections cleanly.

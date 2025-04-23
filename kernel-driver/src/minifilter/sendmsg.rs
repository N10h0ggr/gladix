//! Kernel-to-user communication for file events via FltSendMessage.
//!
//! This module handles sending structured file operation events to the
//! user-agent process. It builds messages based on minifilter context and
//! uses Filter Manager APIs to transmit them securely.
//!
//! Key responsibilities:
//! - Format and serialize telemetry events from the minifilter.
//! - Send data using `FltSendMessage` to user space listener.
//! - Handle timeouts and failures gracefully.
//! - Maintain lightweight queueing/buffering when needed.

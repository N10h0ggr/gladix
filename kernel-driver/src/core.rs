//! Core kernel-mode driver logic and communication handlers.
//!
//! This module handles the core functionality of the driver, including
//! processing configuration updates and providing a communication bridge
//! between kernel-space and user-space components of the EDR.
//!
//! Key responsibilities:
//! - Initialize and dispatch IOCTL interfaces for user-space control.
//! - Handle ring buffer setup (if used) for telemetry events.
//! - Manage configuration received from user agent.
//! - Track driver-wide state and service lifecycle helpers.
//!
//! This module is called during `DriverEntry` for early setup and cleanup.

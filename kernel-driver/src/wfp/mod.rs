//! Windows Filtering Platform (WFP) network inspection subsystem.
//!
//! This module serves as the entry point for the driver's network layer.
//! It sets up registration with WFP and manages the flow of events from
//! connection attempts, stream initiation, or other ALE-layer filters.
//!
//! Key responsibilities:
//! - Register WFP callouts at the ALE flow layer.
//! - Correlate network activity with process information.
//! - Relay selected flow data to user space.
//! - Manage callout lifecycle (register/unregister).

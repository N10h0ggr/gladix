//! gRPC interface between the GUI and the user-agent.
//!
//! This module defines the gRPC service used by the GUI to communicate with
//! the local user-agent. It enables actions like triggering manual scans,
//! querying alert history, or adjusting runtime configurations.
//!
//! Key responsibilities:
//! - Define protobuf service interface and message types.
//! - Handle gRPC server initialization and connection lifecycle.
//! - Dispatch incoming requests to appropriate agent subsystems.

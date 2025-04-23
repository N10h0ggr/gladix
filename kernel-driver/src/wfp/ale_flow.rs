//! ALE_FLOW_ESTABLISHED callout for network flow monitoring.
//!
//! This module implements the core logic of the WFP callout triggered when
//! new network flows are authorized. It extracts flow metadata (IPs, ports,
//! PIDs) and prepares telemetry records for user-mode analysis.
//!
//! Key responsibilities:
//! - Receive callbacks for authorized connections.
//! - Extract source/destination and application context.
//! - Format and send event data to the user-agent component.
//! - Maintain temporary process-flow mapping to avoid stale PID resolution.

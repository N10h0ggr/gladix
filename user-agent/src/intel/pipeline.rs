//! Detection pipeline and alert prioritization engine.
//!
//! This module implements the main event processing flow inside the
//! user-agent's detection logic. It receives telemetry events and applies
//! rule-based or heuristic checks, prioritizing alerts for quick response.
//!
//! Key responsibilities:
//! - Maintain in-memory process and event context.
//! - Apply detection rules to incoming telemetry events.
//! - Assign priority levels to detections (e.g., critical, low).
//! - Dispatch alerts to response mechanisms or the GUI.

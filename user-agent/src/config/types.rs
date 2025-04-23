//! Configuration structures and risk-level definitions.
//!
//! This module defines all configuration data structures used at both
//! deserialization and runtime stages of the agent. It distinguishes between
//! raw config formats (from TOML) and internal representations used during
//! execution.
//!
//! Key responsibilities:
//! - Define risk levels (`DirectoryRisk`) for directory groups.
//! - Represent runtime scan groups (`RiskGroup`).
//! - Define TOML-deserialized structures (`ScannerConfig`, `MasterConfig`, etc.).
//! - Keep a clean separation between config file format and logic-layer usage.

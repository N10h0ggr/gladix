//! Mock tests for kernel driver components in a user-mode environment.
//!
//! Since direct unit testing in kernel mode is impractical, this module
//! provides user-space mocks and stubs to validate logic for structures,
//! serialization, and messaging formats used by the driver.
//!
//! Key responsibilities:
//! - Simulate IOCTL request/response cycles.
//! - Validate message formats sent via FltSendMessage.
//! - Mock process and network events for handler testing.
//! - Ensure correct packing/unpacking of telemetry structures.
//!
//! Note: This module does not interact with a live kernel driver.
//! It is meant for offline validation and pre-integration testing.

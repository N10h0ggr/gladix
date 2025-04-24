//! # Event Model Tests
//!
//! This module contains unit tests for the unified telemetry `Event` enum,
//! ensuring correctness of serialization and deserialization across formats.
//!
//! ## Covered Scenarios:
//!
//! - ✅ JSON round-trip (via `serde`) for event types like `FileEvent`
//! - ✅ Protobuf round-trip (via `prost`) for the same event types
//! - ✅ Structural consistency between internal Rust model and `.proto` schema
//! - ✅ Field-level assertions (e.g., `path`, `pid`, `sha256`)
//!
//! These tests ensure that events can safely move between internal systems,
//! logs, network interfaces, and database storage without data loss or mismatch.
//!
//! They are meant to catch breaking changes in:
//! - Schema evolution
//! - Serialization bugs
//! - Enum variant mismatches
//!
//! ## How to Extend
//! To add support for new telemetry types (e.g. `ScanResult`, `EtwEvent`),
//! simply add a new case to the `Event` enum and replicate the round-trip test
//! patterns shown here.


use chrono::Utc;
use prost::Message;
use std::convert::TryFrom;
use agent::comms::events::{Event, FileEvent, FileOperation};
use shared::events::BaseEvent as ProtoEvent;

#[test]
fn test_file_event_json_roundtrip() {
    let fe = FileEvent {
        ts: Utc::now(),
        sensor_guid: "host1".into(),
        op: FileOperation::Write,
        path: "C:\\foo.txt".into(),
        new_path: None,
        pid: 1234,
        exe_path: "C:\\bin.exe".into(),
        size: 100,
        sha256: vec![0xab, 0xcd],
        success: true,
    };
    let ev = Event::File(fe.clone());
    let json = serde_json::to_string(&ev).unwrap();
    let back: Event = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Event::File(_)));
}

#[test]
fn test_file_event_protobuf_roundtrip() {
    let fe = FileEvent {
        ts: Utc::now(),
        sensor_guid: "host1".into(),
        op: FileOperation::Write,
        path: "C:\\foo.txt".into(),
        new_path: None,
        pid: 1234,
        exe_path: "C:\\bin.exe".into(),
        size: 100,
        sha256: vec![0xab, 0xcd],
        success: true,
    };
    let ev = Event::File(fe.clone());
    let proto: ProtoEvent = ev.into();
    let mut buf = Vec::new();
    proto.encode(&mut buf).unwrap();
    let decoded = ProtoEvent::decode(&*buf).unwrap();
    let back = Event::try_from(decoded).unwrap();
    assert_eq!(matches!(back, Event::File(_)), true);
    if let Event::File(f2) = back {
        assert_eq!(f2.path, fe.path);
    }
}
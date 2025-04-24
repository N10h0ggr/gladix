//! Unified event model used across the agent.
//!
//! This module defines the `Event` enum and associated data structures
//! representing telemetry from all sources: filesystem, network, process
//! creation, scan results, and ETW events.
//!
//! Each event can be serialized to JSON (for logging or persistence)
//! and to Protobuf (for gRPC/IPC communication), and is compatible with the
//! definitions in the shared `event.proto` schema.
//!
//! ## Purpose
//! These types are used to:
//! - Normalize incoming data into a central pipeline
//! - Persist events to SQLite
//! - Trigger detection logic
//! - Render in the UI or export externally
//!
//! ## Formats Supported
//! - `serde` for JSON serialization
//! - `prost` for Protobuf compatibility
//!
//! ## Extension
//! To add new telemetry types, simply:
//! 1. Add a new variant to `Event`
//! 2. Define a new struct with `Serialize`, `Deserialize`
//! 3. Update the `.proto` schema and implement conversion logic


use prost::Message;
use serde::{Deserialize, Serialize};
use anyhow::{anyhow, Error as AnyhowError};
use chrono::{DateTime, Utc, NaiveDateTime};
use core::error::Error;
use crate::proto_gen::agent_telemetry::base_event::Payload;

use crate::proto_gen::agent_telemetry::{
    BaseEvent as ProtoEvent,
    FileEvent as ProtoFileEvent,
    base_event,
    file_event,
};

/// Unified telemetry event type used across the system.
///
/// Each variant corresponds to a different sensor (filesystem, network, process, scan, ETW),
/// and wraps a specific structured payload. This enum enables consistent processing,
/// storage, filtering, and detection across the pipeline.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum Event {
    File(FileEvent),
    Network(NetworkEvent),
    Process(ProcessEvent),
    Scan(ScanResult),
    Etw(EtwEvent),
}

/// Represents a file system operation intercepted by the kernel minifilter.
///
/// This includes actions like creation, writing, deletion, or renaming of files,
/// along with metadata about the process responsible, the file path, and
/// the hash of the content if available.
///
/// These events are often the first signal in execution chains such as
/// droppers, installers, or ransomware attempts.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEvent {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub op: FileOperation,
    pub path: String,
    pub new_path: Option<String>,
    pub pid: u32,
    pub exe_path: String,
    pub size: u64,
    pub sha256: Vec<u8>,
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FileOperation { Create, Write, Delete, Rename }

/// Captures outbound or inbound network flow telemetry as seen by the WFP sensor.
///
/// Includes source/destination addresses and ports, process metadata, and byte counts.
/// These events are useful to detect C2 beacons, data exfiltration, or suspicious traffic
/// to rare destinations.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkEvent {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub direction: Direction,
    pub proto: String,
    pub src_ip: String,
    pub src_port: u32,
    pub dst_ip: String,
    pub dst_port: u32,
    pub pid: u32,
    pub exe_path: String,
    pub bytes: u64,
    pub blocked: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Direction { Inbound, Outbound }

/// Represents the creation of a new process, captured via ETW or kernel callbacks.
///
/// Includes process ID, parent process ID, the executable image, and full command line.
/// Useful for detecting suspicious process chains, LOLBins, or execution anomalies.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessEvent {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub pid: u32,
    pub ppid: u32,
    pub image_path: String,
    pub cmdline: String,
}

/// Result of scanning a file using embedded detection rules (e.g. YARA).
///
/// Includes the triggered rule ID, list of matches, file path, and an associated severity level.
/// This is a detection-level event that may trigger alerts or automated responses.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanResult {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub rule_id: String,
    pub file_path: String,
    pub matches: Vec<String>,
    pub severity: Severity,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Severity { Low, Medium, High, Critical }

/// Raw system telemetry received from ETW (Event Tracing for Windows).
///
/// This is a generic wrapper that supports multiple provider types, preserving
/// the original payload in JSON form for flexible downstream parsing.
/// Examples include module loads, registry changes, or service activity.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EtwEvent {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub provider_guid: String,
    pub event_id: u32,
    pub level: u32,
    pub pid: u32,
    pub tid: u32,
    pub json_payload: String,
}

/// Converts a unified `Event` enum into a `ProtoEvent` (Protobuf-compatible format).
///
/// This is used to send events across gRPC or IPC boundaries by transforming the
/// internal Rust representation into the serialized wire format defined in `event.proto`.
///
/// Each variant of the `Event` enum is mapped to the corresponding `oneof` payload
/// inside the `BaseEvent` Protobuf message.
///
/// # Used By
/// - gRPC endpoint responses
/// - Kernel-to-user IPC forwarding
/// - Logging to remote systems

impl Into<ProtoEvent> for Event {
    fn into(self) -> ProtoEvent {
        match self {
            Event::File(fe) => {
                let mut base = ProtoEvent::default();
                // timestamp
                let ts_proto = prost_types::Timestamp {
                    seconds: fe.ts.timestamp(),
                    nanos: fe.ts.timestamp_subsec_nanos() as i32,
                };
                base.ts = Some(ts_proto);
                base.sensor_guid = fe.sensor_guid.clone();

                // map operation to prost enum
                let op_enum = match fe.op {
                    FileOperation::Create => file_event::Operation::Create,
                    FileOperation::Write => file_event::Operation::Write,
                    FileOperation::Delete => file_event::Operation::Delete,
                    FileOperation::Rename => file_event::Operation::Rename,
                } as i32;

                // set payload
                base.payload = Some(Payload::FileEvent(ProtoFileEvent {
                    op: op_enum,
                    path: fe.path,
                    new_path: fe.new_path.unwrap_or_default(),
                    pid: fe.pid,
                    exe_path: fe.exe_path,
                    size: fe.size,
                    sha256: fe.sha256,
                    success: fe.success,
                }));
                base
            }
            _ => unimplemented!(),
        }
    }
}

/// Attempts to parse a Protobuf `ProtoEvent` into the internal `Event` enum.
///
/// This conversion is fallible to handle edge cases such as:
/// - Missing required fields (e.g., timestamp)
/// - Invalid or unsupported payloads
/// - Legacy or corrupted messages
///
/// This is primarily used when receiving data over gRPC, loading from disk,
/// or parsing incoming telemetry from lower layers.
///
/// # Errors
/// Returns an error if the timestamp is missing or the payload cannot be parsed.

impl TryFrom<ProtoEvent> for Event {
    type Error = AnyhowError;

    fn try_from(pe: ProtoEvent) -> Result<Self, Self::Error> {
        // timestamp conversion
        let ts_proto = pe.ts.ok_or_else(|| anyhow!("missing timestamp"))?;
        let naive = NaiveDateTime::from_timestamp_opt(ts_proto.seconds, ts_proto.nanos as u32)
            .ok_or_else(|| anyhow!("invalid timestamp"))?;
        let ts = DateTime::<Utc>::from_utc(naive, Utc);
        let sensor = pe.sensor_guid;

        if let Some(payload) = pe.payload {
            match payload {
                Payload::FileEvent(f) => {
                    // map prost enum back to local
                    let proto_op = file_event::Operation::from_i32(f.op)
                        .ok_or_else(|| anyhow!("invalid operation"))?;
                    let op = match proto_op {
                        file_event::Operation::Create => FileOperation::Create,
                        file_event::Operation::Write => FileOperation::Write,
                        file_event::Operation::Delete => FileOperation::Delete,
                        file_event::Operation::Rename => FileOperation::Rename,
                    };
                    Ok(Event::File(FileEvent {
                        ts,
                        sensor_guid: sensor,
                        op,
                        path: f.path,
                        new_path: if f.new_path.is_empty() { None } else { Some(f.new_path) },
                        pid: f.pid,
                        exe_path: f.exe_path,
                        size: f.size,
                        sha256: f.sha256,
                        success: f.success,
                    }))
                }
                _ => Err(anyhow!("unsupported payload type")),
            }
        } else {
            Err(anyhow!("no payload"))
        }
    }
}
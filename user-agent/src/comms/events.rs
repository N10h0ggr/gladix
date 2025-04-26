// src/scanner/events.rs

//! Unified event model used across the agent.
//!
//! Defines the `Event` enum and data structures for filesystem, network,
//! process creation, scan results, and ETW telemetry.
//! Supports JSON (serde) and Protobuf (prost) serialization compatible with `event.proto`.

use serde::{Deserialize, Serialize};
use anyhow::{anyhow, Error as AnyhowError};
use chrono::{DateTime, Utc};
use shared::events::base_event::Payload;

use shared::events::{
    BaseEvent as ProtoEvent,
    FileEvent as ProtoFileEvent,
    file_event,
};

/// Core enum representing all telemetry types in a normalized form.
/// Allows pipeline stages (storage, filtering, detection) to work generically.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum Event {
    File(FileEvent),
    Network(NetworkEvent),
    Process(ProcessEvent),
    Scan(ScanResult),
    Etw(EtwEvent),
}

/// File system operations like create, write, delete, rename.
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

/// Network flow events for C2, exfiltration detection.
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

/// Process creation events for detecting anomalous chains.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessEvent {
    pub ts: DateTime<Utc>,
    pub sensor_guid: String,
    pub pid: u32,
    pub ppid: u32,
    pub image_path: String,
    pub cmdline: String,
}

/// Output of embedded scanner (e.g. YARA), triggers alerts.
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

/// Generic ETW wrapper preserving raw JSON payload.
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

/// Convert internal `Event` to Protobuf `ProtoEvent` for gRPC/IPC.
/// Only `File` variant shown; other variants require similar mapping.
impl From<Event> for ProtoEvent {
    fn from(evt: Event) -> ProtoEvent {
        match evt {
            Event::File(fe) => {
                let mut base = ProtoEvent::default();
                // Convert chrono timestamp to prost Timestamp
                let ts_proto = prost_types::Timestamp {
                    seconds: fe.ts.timestamp(),
                    nanos: fe.ts.timestamp_subsec_nanos() as i32,
                };
                base.ts = Some(ts_proto);
                base.sensor_guid = fe.sensor_guid.clone();

                // Map Rust enum into Prost enum values
                let op_enum = match fe.op {
                    FileOperation::Create => file_event::Operation::Create,
                    FileOperation::Write => file_event::Operation::Write,
                    FileOperation::Delete => file_event::Operation::Delete,
                    FileOperation::Rename => file_event::Operation::Rename,
                } as i32;

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
            _ => unimplemented!("Other Event variants not yet implemented"),
        }
    }
}

/// Parse Protobuf `ProtoEvent` back into internal `Event`, verifying required fields.
impl TryFrom<ProtoEvent> for Event {
    type Error = AnyhowError;

    fn try_from(pe: ProtoEvent) -> Result<Self, Self::Error> {
        // Ensure timestamp present and valid
        let ts_proto = pe.ts.ok_or_else(|| anyhow!("missing timestamp"))?;
        let ts = DateTime::from_timestamp(ts_proto.seconds, ts_proto.nanos as u32).expect("invalid timestamp");
        let sensor = pe.sensor_guid;

        match pe.payload.ok_or_else(|| anyhow!("no payload"))? {
            Payload::FileEvent(f) => {
                // TryFrom ensures only valid enum values accepted
                let proto_op = file_event::Operation::try_from(f.op)
                    .map_err(|_| anyhow!("invalid file operation {}", f.op))?;
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
            other => Err(anyhow!("unsupported payload type: {:?}", other)),
        }
    }
}

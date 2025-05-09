//! `comms` – communication primitives
//! ----------------------------------
//! * `memory_ring`  → zero‑copy ring buffer shared with the kernel driver.
//! * `router`       → splits BaseEvents coming from the ring.
//! * `Buses`        → fan‑out channels (DB writer + realtime broadcast).
//! * `WrappedEvent` → uniform envelope used by all internal pipelines.
pub mod memory_ring;

use prost_types::Timestamp;
use tokio::sync::{broadcast, mpsc};

/// Uniform wrapper so every pipeline sees `{ts, sensor_guid, payload}`
/// independent of the concrete protobuf message type.
#[derive(Clone)]
pub struct WrappedEvent<E: Clone>  {
    pub ts:          Timestamp,
    pub sensor_guid: String,
    pub payload:     E,
}

#[derive(Clone)]
pub struct TokioBuses<E: Clone + Send + 'static> {
    pub db_tx:    mpsc::Sender<WrappedEvent<E>>,
    pub intel_tx: broadcast::Sender<WrappedEvent<E>>,
}

impl<E: Clone + Send + 'static> TokioBuses<E> {
    pub fn new(db_capacity: usize, intel_capacity: usize) -> Self {
        let (db_tx, _)    = mpsc::channel(db_capacity);
        let (intel_tx, _) = broadcast::channel(intel_capacity);
        Self { db_tx, intel_tx }
    }
}


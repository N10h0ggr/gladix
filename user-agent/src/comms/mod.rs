
pub mod memory_ring;
pub mod router;

use prost_types::Timestamp;
use tokio::sync::{broadcast, mpsc};

/// Asegúrate de añadir este derive para que luego WrappedEvent<E>: Clone
#[derive(Clone)]
pub struct WrappedEvent<E: Clone>  {
    pub ts:          Timestamp,
    pub sensor_guid: String,
    pub payload:     E,
}

#[derive(Clone)]
pub struct Buses<E: Clone + Send + 'static> {
    pub db_tx:    mpsc::Sender<WrappedEvent<E>>,
    pub intel_tx: broadcast::Sender<WrappedEvent<E>>,
}

impl<E: Clone + Send + 'static> Buses<E> {
    pub fn new(db_capacity: usize, intel_capacity: usize) -> Self {
        let (db_tx, _)    = mpsc::channel(db_capacity);
        let (intel_tx, _) = broadcast::channel(intel_capacity);
        Self { db_tx, intel_tx }
    }
}


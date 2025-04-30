pub mod events;
pub mod listeners;
pub mod memory_ring;

use prost_types::Timestamp;

/// Asegúrate de añadir este derive para que luego WrappedEvent<E>: Clone
#[derive(Clone)]
pub struct WrappedEvent<E: Clone>  {
    pub ts:          Timestamp,
    pub sensor_guid: String,
    pub payload:     E,
}


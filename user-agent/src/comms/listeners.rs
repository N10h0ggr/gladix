// src/comms/listeners.rs

use std::{marker::PhantomData, sync::Arc, time::SystemTime};
use async_trait::async_trait;
use prost::Message;
use tokio::{task, sync::{broadcast, mpsc}};

use super::{WrappedEvent, memory_ring::MemoryRing};

/// Canales para enviar WrappedEvent<E> a base de datos e inteligencia.
/// E: Clone + Send + 'static asegura que WrappedEvent<E> sea Clone + Send + 'static.
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

/// Trait genérico de listeners que producen WrappedEvent<E>.
/// E: Clone + Send + 'static para que los canales y futures sean Send + 'static.
#[async_trait]
pub trait Listener<E: Clone + Send + 'static>: Send + Sync + 'static {
    /// Nombre para logs/metrics.
    fn name(&self) -> &'static str;

    /// Capacidad del canal interno “raw”.
    fn capacity(&self) -> usize { 16_384 }

    /// Lee del ring, decodifica E y envuelve en WrappedEvent<E>.
    async fn ingest(self: Arc<Self>, tx: mpsc::Sender<WrappedEvent<E>>);

    /// Filtro/triage opcional (por defecto pasa todo).
    fn triage(&self, ev: WrappedEvent<E>) -> Option<WrappedEvent<E>> {
        Some(ev)
    }

    /// Helper que lanza ingest + triage → broadcast + db.
    fn spawn(self: Arc<Self>, buses: Buses<E>) {
        let name = self.name();
        let cap  = self.capacity();
        let (raw_tx, mut raw_rx) = mpsc::channel::<WrappedEvent<E>>(cap);
        let ingest_self = self.clone();
        let triage_self = self;
        let Buses { db_tx, intel_tx } = buses;

        // Tarea de ingest
        task::spawn(async move {
            log::info!("listener '{}' ingest started", name);
            ingest_self.ingest(raw_tx).await;
            log::info!("listener '{}' ingest ended", name);
        });

        // Tarea de triage + forward
        task::spawn(async move {
            log::info!("listener '{}' triage started", name);
            while let Some(ev) = raw_rx.recv().await {
                if let Some(ev2) = triage_self.triage(ev) {
                    // clonamos para intel; el original va a BD
                    let _ = intel_tx.send(ev2.clone());
                    let _ = db_tx.send(ev2).await;
                }
            }
            log::info!("listener '{}' triage ended", name);
        });
    }
}

/// Listener que lee bytes de un MemoryRing, los decodifica con prost y envuelve.
pub struct RingListener<E> {
    name:        &'static str,
    ring:        MemoryRing,
    sensor_guid: String,
    _marker:     PhantomData<E>,
}

impl<E> RingListener<E> {
    /// `name`: p.ej. "network"; `ring`: tu MemoryRing; `sensor_guid`: desde config.
    pub fn new(
        name: &'static str,
        ring: MemoryRing,
        sensor_guid: impl Into<String>
    ) -> Self {
        Self {
            name,
            ring,
            sensor_guid: sensor_guid.into(),
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<E> Listener<E> for RingListener<E>
where
// E debe decodificarse con prost, clonarse, enviarse entre hilos y vivir 'static
    E: Message + Default + Clone + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }

    async fn ingest(self: Arc<Self>, tx: mpsc::Sender<WrappedEvent<E>>) {
        loop {
            match self.ring.pop().await {
                Some(bytes) => match E::decode(&*bytes) {
                    Ok(payload) => {
                        let wrapped = WrappedEvent {
                            // SystemTime::now() se convierte a prost_types::Timestamp
                            ts:          SystemTime::now().into(),
                            sensor_guid: self.sensor_guid.clone(),
                            payload,
                        };
                        if tx.send(wrapped).await.is_err() {
                            // receptor cerrado → salimos
                            break;
                        }
                    }
                    Err(err) => {
                        log::error!("listener '{}': decode error: {:?}", self.name, err);
                    }
                },
                None => {
                    // buffer cerrado
                    break;
                }
            }
        }
    }
}

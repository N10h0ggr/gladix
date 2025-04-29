//! Listener abstraction + concrete stubs (gRPC, ETW, ring‑buffer).
//! -----------------------------------------------------------------------------
//! A **listener** ingests raw telemetry from one protocol / source and hands it
//! to a small **triage** task that forwards it to the global buses:
//!   • `intel_tx.send(ev.clone())` → unbounded multicast for the detection
//!     engine.
//!   • `db_tx.try_send(ev)`       → bounded queue in front of the single DB
//!     writer (loss‑tolerant).
//!
//! We keep one triage per listener so bursty sources cannot starve others.  This
//! file only contains skeletons that *compile*; real decoding / gRPC plumbing
//! will be added incrementally.

use std::{sync::Arc, time::Duration};
use tokio::{sync::{broadcast, mpsc}, task, time};
use async_trait::async_trait;
use crossbeam::channel::Receiver as CbReceiver;

use crate::comms::events::{Event, ProcessEvent};

// ============================================================================
// 0 ▸ Global buses (intel + DB)
// ============================================================================

/// Shared send handles cloned into every triage task.
#[derive(Clone)]
pub struct Buses {
    pub intel_tx: broadcast::Sender<Event>,
    pub db_tx:    mpsc::Sender<Event>,
}

impl Buses {
    pub fn new(db_capacity: usize, broadcast_capacity: usize) -> Self {
        let (db_tx,  _) = mpsc::channel(db_capacity);
        let (intel_tx, _) = broadcast::channel(broadcast_capacity);
        Self { intel_tx, db_tx }
    }
}

// ============================================================================
// 1 ▸ Listener trait – uniform way to spawn them
// ============================================================================

#[async_trait]
pub trait Listener: Send + Sync + 'static {
    /// Display name for metrics / logs.
    fn name(&self) -> &'static str;

    /// Capacity of the per‑listener raw queue.
    fn capacity(&self) -> usize { 16_384 }

    /// Spawn the I/O loop that pulls data from the external source and pushes
    /// `Event`s into the provided `tx`.
    async fn ingest(self: Arc<Self>, tx: mpsc::Sender<Event>);

    /// Optional triage step – default is pass‑through.
    fn triage(&self, ev: Event) -> Option<Event> { Some(ev) }

    /// Convenience helper: launches *ingest* + *triage*.
    fn spawn(self: Arc<Self>, buses: Buses) {
        let name = self.name();
        let cap  = self.capacity();

        // Channel between ingest task and triage task.
        let (raw_tx, mut raw_rx) = mpsc::channel::<Event>(cap);
        let ingest_self = self.clone();
        let triage_self = self;

        // ── Task 1: ingest (protocol‑specific) ────────────────────────────
        task::spawn(async move {
            log::info!("listener '{name}' started");
            ingest_self.ingest(raw_tx).await;
            log::info!("listener '{name}' exited");
        });

        // ── Task 2: triage & forward ─────────────────────────────────────
        let Buses { intel_tx, db_tx } = buses;
        task::spawn(async move {
            while let Some(mut ev) = raw_rx.recv().await {
                if let Some(ev2) = triage_self.triage(ev) {
                    let _ = intel_tx.send(ev2.clone()); // never blocks
                    let _ = db_tx.try_send(ev2);        // may drop if full
                }
            }
            log::info!("triage for '{name}' terminated – chan closed");
        });
    }
}

/// A listener that pulls `ProcessEvent` from a crossbeam channel (standing in
/// for your real shared‐memory ring buffer) and forwards them as `Event::Process`.
pub struct ProcessRingBufferListener {
    rx: CbReceiver<ProcessEvent>,
}

impl ProcessRingBufferListener {
    /// Build from the receiver end of your ring buffer
    pub fn new(rx: CbReceiver<ProcessEvent>) -> Self {
        Self { rx }
    }
}

#[async_trait]
impl Listener for ProcessRingBufferListener {
    fn name(&self) -> &'static str {
        "process_ring_buffer"
    }

    async fn ingest(self: Arc<Self>, tx: mpsc::Sender<Event>) {
        // Offload the blocking recv loop to a dedicated OS thread
        let rx = self.rx.clone();
        task::spawn_blocking(move || {
            while let Ok(pe) = rx.recv() {
                let ev = Event::Process(pe);
                // blocking_send() will block _this_ thread only, never a Tokio worker
                if tx.blocking_send(ev).is_err() {
                    // downstream dropped → exit
                    break;
                }
            }
        })
            .await
            .expect("process ring ingest task panicked");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comms::listeners::Buses;
    use crate::comms::events::Event;
    use chrono::Utc;
    use crossbeam::channel::unbounded;
    use tokio::sync::{broadcast, mpsc};

    #[tokio::test]
    async fn test_process_ring_listener() {
        // 1) prepare a fake ring-buffer (crossbeam channel) and two events
        let (rb_tx, rb_rx) = unbounded();
        let pe1 = ProcessEvent {
            ts: Utc::now(),
            sensor_guid: "sensor-A".into(),
            pid: 42,
            ppid: 1,
            image_path: "/usr/bin/foo".into(),
            cmdline: "--foo".into(),
        };
        let pe2 = ProcessEvent {
            ts: Utc::now(),
            sensor_guid: "sensor-B".into(),
            pid: 99,
            ppid: 42,
            image_path: "/usr/bin/bar".into(),
            cmdline: "--bar".into(),
        };

        // 2) build listener + buses
        let listener = Arc::new(ProcessRingBufferListener::new(rb_rx));
        let (db_tx, _db_rx) = mpsc::channel::<Event>(8);
        let (intel_tx, mut intel_rx) = broadcast::channel::<Event>(8);
        let buses = Buses { intel_tx: intel_tx.clone(), db_tx };

        // 3) spawn the two tasks (ingest + triage)
        listener.spawn(buses);

        // 4) push into the “ring buffer”
        rb_tx.send(pe1.clone()).unwrap();
        rb_tx.send(pe2.clone()).unwrap();

        // 5) give the background tasks a moment
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 6) assert we saw both on the intel channel
        let mut got = Vec::new();
        for _ in 0..2 {
            if let Ok(Event::Process(pe)) = intel_rx.recv().await {
                got.push(pe);
            }
        }
        assert_eq!(got.len(), 2);
        assert!(got.contains(&pe1));
        assert!(got.contains(&pe2));
    }
}
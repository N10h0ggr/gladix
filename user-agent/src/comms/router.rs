//! Ring‑buffer router
//! ==================
//! *Single* blocking task that drains the kernel’s memory ring,
//! deserialises each `BaseEvent`, and re‑sends it to the appropriate
//! `Buses<T>` for DB‑persistence and/or intel fan‑out.
//
//! Transport design
//! ─────────────────
//!   • **Ring buffer** ← kernel callback thread
//!       ↳ only *early‑life‑cycle* events that cannot be emitted by a
//!         minifilter (`FileEvent`) or normal Win32/ETW APIs.
//!         Today that is: `ProcessEvent` (create/exit).
//!         Soon: `ImageLoadEvent`, `ObjectOpEvent` (handle dup/open…).
//!
//!   • **FilterSendMessage()**  ← mini‑filter (already handled elsewhere)
//!   • **ETW + Win32 APIs**     ← user‑mode collectors (elsewhere)
//
//! This router therefore handles a **subset** of the protobuf schema.
//! Unknown payloads are ignored (they’ll arrive through other paths).

use std::{sync::Arc, time::SystemTime};

use prost::Message;
use tokio::task;

use shared::events::{
    base_event::Payload, BaseEvent, ProcessEvent,
    /* Place‑holders for the next driver milestones; keep the `use`
       so the compiler reminds us to implement them when ready. */
    ImageLoadEvent,  // ← define in .proto when you add the callback
    ObjectOpEvent,   // ← (e.g. “handle opened to LSASS”)
};
use crate::comms::{memory_ring::MemoryRing, WrappedEvent};
use crate::comms::Buses;

/*──────────────────────────── public API ────────────────────────────────*/

/// Owns one `Buses<T>` for every payload that *can* come from the ring.
///
/// * Not all fields are mandatory today – you can pass `None` until the
///   corresponding DB/intel pipeline exists.
/// * Makes adding new ring‑sourced events a one‑liner.
pub struct KernelRingRouter {
    ring: MemoryRing,

    proc_bus:   Option<Buses<ProcessEvent>>,
    img_bus:    Option<Buses<ImageLoadEvent>>,
    obj_bus:    Option<Buses<ObjectOpEvent>>,
}

impl KernelRingRouter {
    pub fn new(
        ring: MemoryRing,
        proc_bus:   Option<Buses<ProcessEvent>>,
        img_bus:    Option<Buses<ImageLoadEvent>>,
        obj_bus:    Option<Buses<ObjectOpEvent>>,
    ) -> Self {
        Self { ring, proc_bus, img_bus, obj_bus }
    }

    /// Spawn the blocking reader on a dedicated OS thread so deserialisation
    /// doesn’t interfere with Tokio’s async runtime.
    pub fn spawn(self: Arc<Self>) {
        task::spawn_blocking(move || {
            log::info!("ring‑router task started");
            loop {
                match self.ring.next() {
                    Some(buf) => match BaseEvent::decode(&*buf) {
                        Ok(evt) => self.dispatch(evt),
                        Err(e)  => log::error!("ring decode error: {:?}", e),
                    },
                    None => std::thread::sleep(std::time::Duration::from_millis(2)),
                }
            }
        });
    }

    /*──────────────────────── private helpers ─────────────────────────*/

    fn dispatch(&self, evt: BaseEvent) {
        let ts   = evt.ts.unwrap_or_else(|| SystemTime::now().into());
        let guid = evt.sensor_guid; // stays the same for every payload

        match evt.payload {
            Some(Payload::ProcessEvent(p))   =>
                push(&self.proc_bus, ts, &guid, p),

            Some(Payload::ImageLoadEvent(i)) =>
                push(&self.img_bus,  ts, &guid, i),

            Some(Payload::ObjectOpEvent(o))  =>
                push(&self.obj_bus,  ts, &guid, o),

            // Any other payload here means the driver sent something it
            // *should* have sent through a different channel, or the schema
            // evolved and this router wasn’t updated yet.
            _ => log::debug!("ring payload ignored (no handler)"),
        }
    }
}

/*──────────────────────── generic fan‑out ───────────────────────────────*/
/// Send to intel first (fire‑and‑forget), then try the DB channel.
/// If the corresponding `Buses` is not wired yet, just drop the event.
fn push<E: Clone + Send + 'static>(
    maybe_buses: &Option<Buses<E>>,
    ts: prost_types::Timestamp,
    guid: &str,
    payload: E,
) {
    if let Some(buses) = maybe_buses {
        let wrapped = WrappedEvent { ts, sensor_guid: guid.to_owned(), payload };
        let _ = buses.intel_tx.send(wrapped.clone());  // ignore on shutdown
        let _ = buses.db_tx.try_send(wrapped);
    }
}

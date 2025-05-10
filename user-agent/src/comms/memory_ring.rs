//! Memory‑mapped ring‑buffer reader
//! --------------------------------
//! The kernel driver exposes a named section (`Global\MySharedSection`)
//! that contains a *single‑producer / single‑consumer* byte ring:
//
//!   ┌──── 0                             size (64 KiB) ────┐
//!   │ u32 write_off │   len | msg   len | msg   …        │
//!   └─────────────────────────────────────────────────────┘
//
//! `write_off` **always** moves forward; the writer wraps to 4 whenever the
//!  next message would cross the buffer end.
//! The kernel writes events as: [u32 length][protobuf payload]
//! The reader skips 4 bytes and decodes the payload with `prost`.
//! Reader keeps its own cursor (`READ_OF`, in process memory).
//!
//! This file maps the section READ‑ONLY and provides `next()` which returns the
//! next complete protobuf blob (if any).

use std::{
    ffi::OsStr,
    io,
    os::windows::prelude::OsStrExt,
    ptr,
    slice,
    sync::atomic::{AtomicU32, Ordering},
};
use log::{info, debug, error, warn};
use std::time::Duration;
use metrics::{counter, histogram};
use tokio::runtime::Runtime;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::Memory::{
        OpenFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ,
        MEMORY_MAPPED_VIEW_ADDRESS,
    },
};
use shared::events::{BaseEvent, EtwEvent, FileEvent, NetworkEvent, ProcessEvent};
use shared::events::{base_event::Payload};
use shared::constants::USER_SHARED_SECTION_NAME;
use prost::Message;
use prost_types::Timestamp;
use tokio::time::sleep;
use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::System::Memory::FILE_MAP_ALL_ACCESS;

use super::{TokioBuses, WrappedEvent};


/* ───────────────────────── Ring mapping ─────────────────────────── */

#[repr(C)]
struct Header {
    head:    AtomicU32,
    tail:    AtomicU32,
    dropped: AtomicU32,
    size:    u32,
}

/// Single‑producer / single‑consumer ring reader.
/// Safe because we keep one instance per task.
pub struct MemoryRing {
    handle: HANDLE,
    view:   MEMORY_MAPPED_VIEW_ADDRESS,
    hdr:    *mut Header,
    data:   *mut u8,
    size:   u32,
    tail:   u32,
}

unsafe impl Send for MemoryRing {}
unsafe impl Sync for MemoryRing {}

impl MemoryRing {
    pub fn open() -> io::Result<Self> {
        // 1) Log the name we’re about to open
        info!(target: "ring", " Opening shared section `{}`", USER_SHARED_SECTION_NAME);

        // 2) Build a null-terminated UTF-16 version of that name
        let wide: Vec<u16> = OsStr::new(USER_SHARED_SECTION_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        debug!(target: "ring", "   wide name: {:?} (len={})", &wide, wide.len());

        // 3) Call OpenFileMappingW
        let handle: HANDLE = unsafe {
            OpenFileMappingW(
                FILE_MAP_ALL_ACCESS,
                0 as windows_sys::Win32::Foundation::BOOL,            // FALSE for “inherit handle”
                wide.as_ptr().into(), // ensure we get a PCWSTR
            )
        };
        if handle.is_null() {
            let err = io::Error::last_os_error();
            error!(target: "ring", "OpenFileMappingW failed: {:?}", err);
            return Err(err);
        }
        debug!(target: "ring", " OpenFileMappingW OK — handle={:?}", handle);

        // 4) Map the view
        let view = unsafe {
            MapViewOfFile(
                handle,
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                0, // map entire section
            )
        };
        if view.Value.is_null() {
            let err = io::Error::last_os_error();
            error!(target: "ring", " MapViewOfFile failed: {:?}", err);
            unsafe { CloseHandle(handle) };
            return Err(err);
        }
        debug!(target: "ring", " MapViewOfFile OK — ptr={:?}", view.Value);

        // 5) Peel off header & pointers
        let hdr  = view.Value as *mut Header;
        let size = unsafe { (*hdr).size };
        let data = unsafe { (hdr as *mut u8).add(std::mem::size_of::<Header>()) };
        let tail = unsafe { (*hdr).tail.load(Ordering::Acquire) };

        info!(target: "ring", "Shared section opened: size={} tail={}", size, tail);
        Ok(Self { handle, view, hdr, data, size, tail })
    }

    /// Returns the next complete protobuf blob – `None` if producer caught up.
    pub fn next(&mut self) -> Option<Vec<u8>> {
        let head = unsafe { (*self.hdr).head.load(Ordering::Acquire) };
        if self.tail == head {
            return None;
        }

        debug!(
            target: "ring",
            "Reading frame: head={} tail={} buffer_size={}",
            head, self.tail, self.size
        );

        /* 1 ▸ read little‑endian length (may wrap) */
        let len = {
            let mut tmp = [0u8; 4];
            self.copy_circular(self.tail, &mut tmp);
            u32::from_le_bytes(tmp)
        };

        debug!(target: "ring", "Length prefix at offset {}: len={}", self.tail, len);

        if len == 0 || len > self.size - 4 {
            // corruption → resync to producer tail
            warn!(
                target: "ring",
                "Corrupt or invalid frame length detected: len={}, tail={}, head={}. Skipping to head.",
                len, self.tail, head
            );
            self.tail = head;
            unsafe { (*self.hdr).tail.store(self.tail, Ordering::Release) };
            return None;
        }

        /* 2 ▸ full payload already written? */
        let avail = if self.tail <= head {
            head - self.tail
        } else {
            self.size - self.tail + head
        };
        if avail < len + 4 {
            debug!(
                target: "ring",
                "Not enough data yet: needed={} available={}", len + 4, avail
            );
            return None; // producer still writing it
        }

        /* 3 ▸ copy out */
        let mut buf = vec![0u8; len as usize];
        self.copy_circular(self.tail.wrapping_add(4), &mut buf);

        /* 4 ▸ advance + publish new tail */
        self.tail = (self.tail + 4 + len) % self.size;
        unsafe { (*self.hdr).tail.store(self.tail, Ordering::Release) };

        debug!(
            target: "ring",
            "Frame read OK: consumed {} bytes (tail -> {})",
            4 + len,
            self.tail
        );

        Some(buf)
    }

    #[inline]
    fn copy_circular(&self, mut off: u32, dst: &mut [u8]) {
        let size = self.size as usize;
        let data = self.data as *const u8;
        let mut rem = dst.len();
        let mut pos = 0;

        while rem != 0 {
            let chunk = rem.min(size - off as usize);
            unsafe {
                ptr::copy_nonoverlapping(
                    data.add(off as usize),
                    dst.as_mut_ptr().add(pos),
                    chunk,
                );
            }
            rem -= chunk;
            pos += chunk;
            off = (off + chunk as u32) % self.size;
        }
    }
}

impl Drop for MemoryRing {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}

/* ────────────────────── bus fan‑out (macro) ─────────────────────── */

/// One entry per concrete payload:
///     field_name => PayloadVariant( RustType )
///
/// *Add a new line here when you grow the protobuf schema.*
macro_rules! define_memory_ring_buses {
    (
        $( $field:ident => $variant:ident ( $typ:ty ) ),+ $(,)?
    ) => {
        /// `MemoryRingBuses` holds the pair `{db_tx, intel_tx}` for *each* payload.
        #[derive(Clone)]
        pub struct MemoryRingBuses {
            $( pub $field: super::TokioBuses<$typ>, )+
        }

        impl MemoryRingBuses {
            /// Send a fully‑typed `WrappedEvent` to the correct bus.
            async fn dispatch(
                &self,
                ts: Timestamp,
                sensor_guid: String,
                payload: Payload,
            ) {
                match payload {
                    $(
                        Payload::$variant(inner) => {
                            let event = super::WrappedEvent {
                                ts,
                                sensor_guid,
                                payload: inner,
                            };
                             counter!(
                                "events_total",
                                "payload" => stringify!($variant)
                            ).increment(1);
                            // db writer
                            let _ = self.$field.db_tx.send(event.clone()).await;
                            // realtime broadcast
                            let _ = self.$field.intel_tx.send(event);
                        }
                    )+
                    other => {
                        log::warn!("ring: unhandled payload variant {:?}", other);
                    }
                }
            }
        }
    };
}

/*  ←──── Add new event types here ─────────────────────────────────── */
define_memory_ring_buses! {
    process => ProcessEvent(ProcessEvent),
    file    => FileEvent   (FileEvent),
    net     => NetworkEvent(NetworkEvent),
    etw     => EtwEvent    (EtwEvent),
}

/* ─────────────────────── poll‑loop spawner ──────────────────────── */

/// Spawn one Tokio task that
///   • drains *all* currently‑available events in a burst;
///   • sleeps 1ms when the ring is empty (bounded latency, no busy‑loop);
///   • wraps & dispatches each payload to its `TokioBuses`.
pub fn spawn_ring_consumer(
    rt: &Runtime,
    mut ring: MemoryRing,
    buses: MemoryRingBuses,
) {
    rt.spawn(async move {
        loop {
            let mut drained = false;

            while let Some(raw) = ring.next() {
                drained = true;

                match BaseEvent::decode(&*raw) {
                    Ok(evt) => {
                        debug!(target: "ring", "Decoded BaseEvent: {:?}", evt);
                        if let Some(payload) = evt.payload {
                            let ts = evt.ts.unwrap_or_default();
                            buses
                                .dispatch(ts, evt.sensor_guid, payload)
                                .await;
                        } else {
                            log::warn!("ring: BaseEvent without payload");
                        }
                    }
                    Err(e) => {
                        counter!("evt_decode_error_total").increment(1);
                        log::error!(
                            "ring: protobuf decode error: {e}; raw[0..16]={:02x?} (len={})",
                            &raw[..raw.len().min(16)],
                            raw.len()
                        );
                    }
                }
            }

            // TODO: Implement adaptive backoff strategy:
            // - Reset sleep to a low value (e.g., 10 ms) after draining events.
            // - Gradually increase sleep (e.g., exponential backoff) when no events are found.
            // - Cap the sleep duration (e.g., max 1000 ms) to balance latency and CPU usage.
            if !drained {
                sleep(Duration::from_millis(500)).await;
            }
        }
    });
}
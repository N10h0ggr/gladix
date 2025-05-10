//! kernel-driver/src/callbacks/psnotify.rs
//!
//! This kernel-mode module registers a `PsSetCreateProcessNotifyRoutineEx`
//! callback, turns every *process-create* into a `shared::events::ProcessEvent`,
//! wraps it in `BaseEvent`, serialises with `prost`, and pushes a
//! length-prefixed message into the driver’s `MemoryRing`.
//!
//! Framing on the wire now looks like:
//!   [u32 write_offset]
//!   [u32 msg_len][ msg bytes… ]
//!   [u32 msg_len][ msg bytes… ]
//!   …
//! where each `msg_len` is the little-endian byte-length of its following
//! protobuf payload.  This makes the reader’s job trivial: read exactly
//! `msg_len` bytes per `BaseEvent`.
//!
//! We ignore *process-exit* notifications (`info_ptr == NULL`).

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::{ptr, slice};

use prost::Message;
use shared::events::{base_event, BaseEvent, ProcessEvent};
use wdk_sys::{
    HANDLE, LARGE_INTEGER, NTSTATUS, STATUS_SUCCESS, UNICODE_STRING,
    PS_CREATE_NOTIFY_INFO, PEPROCESS,
    ntddk::{KeQuerySystemTimePrecise, PsGetProcessId, PsSetCreateProcessNotifyRoutineEx},
};

use crate::communications::memory_ring::MemoryRing;
use crate::helpers::*;

/*────────────────── constants ─────────────────*/

/// Hard‑coded sensor GUID; load from registry/IoCtl in production.
const SENSOR_GUID: &str = "00000000‑0000‑0000‑0000‑000000000000";

/// Pointer to the ring (written once in [`register`], read thereafter).
static mut RING: *const MemoryRing = ptr::null();

/*────────────────── registration ─────────────────*/

/// Install the callback.
///
/// # Safety
/// Call exactly once during driver initialisation.
pub fn register(ring: &'static MemoryRing) -> Result<(), NTSTATUS> {
    unsafe {
        RING = ring as *const _;
        // SAFETY: parameters match WDK prototype, `Remove = 0` (FALSE).
        let st = unsafe { PsSetCreateProcessNotifyRoutineEx(Some(process_notify), 0u8) };
        if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
    }
}

/// Remove the callback (mirror of [`register`]).
///
/// # Safety
/// Call once during driver unload.
pub unsafe fn unregister() -> Result<(), NTSTATUS> {
    // SAFETY: same call, `Remove = 1` (TRUE).
    let st = unsafe { PsSetCreateProcessNotifyRoutineEx(Some(process_notify), 1u8) };
    if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
}


/*────────────────── callback ─────────────────*/

/// Actual notify routine.
/// Only creation events are processed (`info_ptr != NULL`).
unsafe extern "C" fn process_notify(
    process: PEPROCESS,
    _proc_id: HANDLE,
    info_ptr: *mut PS_CREATE_NOTIFY_INFO,
) {
    /*──── ignore exits ────*/
    if info_ptr.is_null() { return; }

    /*──── gather fields ───*/
    let pid = unsafe { PsGetProcessId(process) as u32 };
    let ppid = unsafe { (*info_ptr).ParentProcessId as u32 };

    let info = unsafe { &*info_ptr };
    let image_path = unsafe { uni_to_string(info.ImageFileName) };
    let cmdline    = unsafe { uni_to_string(info.CommandLine) };

    // Timestamp
    let mut li = LARGE_INTEGER { QuadPart: 0 };
    // SAFETY: kernel API.
    unsafe { KeQuerySystemTimePrecise(&mut li) };
    let ts = li_to_timestamp(li);

    /*──── build protobuf ──*/
    let proc_evt = ProcessEvent { pid, ppid, image_path, cmdline };
    let base_evt = BaseEvent {
        ts: Some(ts),
        sensor_guid: SENSOR_GUID.into(),
        payload: Some(base_event::Payload::ProcessEvent(proc_evt)),
    };

    /*──── encode & push ───*/
    let mut buf = Vec::with_capacity(base_evt.encoded_len() as usize);
    base_evt.encode(&mut buf).unwrap();

    // SAFETY: `RING` was set in `register`; aliased read only.
    let ring = unsafe { RING };
    if !ring.is_null() {
        unsafe { (*ring).push_bytes(&buf) };
    }
}

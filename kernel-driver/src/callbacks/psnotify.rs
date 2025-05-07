//! Process‑creation notify  →  `BaseEvent::ProcessEvent`
//!
//! This kernel‑mode module registers a `PsSetCreateProcessNotifyRoutineEx`
//! callback, turns every *process‑create* into a `shared::events::ProcessEvent`,
//! wraps it in `BaseEvent`, serialises with `prost`, and pushes the raw bytes
//! into the driver’s `MemoryRing`.
//!
//! We ignore *process‑exit* notifications (`info_ptr == NULL`).

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::{ptr, slice};

use prost::Message;
use prost_types::Timestamp;
use shared::events::{base_event, BaseEvent, ProcessEvent};
use wdk_sys::{
    HANDLE, LARGE_INTEGER, NTSTATUS, STATUS_SUCCESS, UNICODE_STRING,
    PS_CREATE_NOTIFY_INFO, PEPROCESS,
    ntddk::{KeQuerySystemTimePrecise, PsGetProcessId, PsSetCreateProcessNotifyRoutineEx},
};

use crate::communications::memory_ring::MemoryRing;

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
        let st = PsSetCreateProcessNotifyRoutineEx(Some(process_notify), 0u8);
        if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
    }
}

/// Remove the callback (mirror of [`register`]).
///
/// # Safety
/// Call once during driver unload.
pub unsafe fn unregister() -> Result<(), NTSTATUS> {
    // SAFETY: same call, `Remove = 1` (TRUE).
    let st = PsSetCreateProcessNotifyRoutineEx(Some(process_notify), 1u8);
    if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
}

/*────────────────── helpers ─────────────────*/

/// Convert a `UNICODE_STRING*` to a Rust `String`.
///
/// # Safety
/// `uni` must be a valid, initialised pointer from the kernel.
unsafe fn uni_to_string(uni: *const UNICODE_STRING) -> String {
    if uni.is_null() {
        return String::new();
    }
    // SAFETY: caller guarantees pointer validity.
    let u = unsafe { &*uni };
    let len = (u.Length / 2) as usize;
    // SAFETY: buffer points to `len` UTF‑16 code units.
    let buf = unsafe { slice::from_raw_parts(u.Buffer, len) };
    String::from_utf16_lossy(buf)
}

/// Convert 100‑ns Windows ticks to protobuf `Timestamp`.
fn li_to_timestamp(li: LARGE_INTEGER) -> Timestamp {
    // SAFETY: union field access.
    let ticks = unsafe { li.QuadPart as i64 };
    const WIN_TO_UNIX_SECS: i64 = 11_644_473_600;
    let secs  = (ticks / 10_000_000) - WIN_TO_UNIX_SECS;
    let nanos = ((ticks % 10_000_000) * 100) as i32;
    Timestamp { seconds: secs, nanos }
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
    // SAFETY: kernel guarantees `process` is valid.
    let pid = unsafe { PsGetProcessId(process) as u32 };
    // Parent PID comes from the info struct.
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
    let mut buf = Vec::with_capacity(base_evt.encoded_len());
    if base_evt.encode(&mut buf).is_err() { return; }

    // SAFETY: `RING` was set in `register`; aliased read only.
    let ring = unsafe { RING };
    if !ring.is_null() {
        unsafe { (*ring).push_bytes(&buf) };
    }
}

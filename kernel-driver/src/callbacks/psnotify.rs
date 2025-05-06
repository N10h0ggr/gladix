//! Process‑creation notify → BaseEvent encoder
#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::{ptr, slice};

use prost::Message;
use prost_types::Timestamp;
use shared::events::{base_event, BaseEvent, ProcessEvent};
use wdk_sys::{
    LARGE_INTEGER, NTSTATUS, STATUS_SUCCESS, UNICODE_STRING,
    ntddk::{
        KE_QUERY_SYSTEM_TIME_PRECISE_NAME, PS_CREATE_NOTIFY_INFO, PEPROCESS,
        KeQuerySystemTimePrecise, PsGetProcessId, PsSetCreateProcessNotifyRoutineEx,
        PsRemoveCreateProcessNotifyRoutineEx,
    },
};

use crate::communications::memory_ring::MemoryRing;

/*---------------------------------------------------------------------------*/
/*  Public constants                                                          */
/*---------------------------------------------------------------------------*/

/// Hard‑coded sensor GUID; swap for registry/IoCtl configuration if needed.
const SENSOR_GUID: &str = "00000000‑0000‑0000‑0000‑000000000000";

/*---------------------------------------------------------------------------*/
/*  Private globals                                                           */
/*---------------------------------------------------------------------------*/

/// Raw pointer to the MemoryRing supplied by the driver
static mut RING: *const MemoryRing = ptr::null();

/*---------------------------------------------------------------------------*/
/*  Registration helpers                                                      */
/*---------------------------------------------------------------------------*/

pub fn register(ring: &'static MemoryRing) -> Result<(), NTSTATUS> {
    unsafe {
        RING = ring as *const _;
        let st = PsSetCreateProcessNotifyRoutineEx(Some(process_notify), /*Remove=*/false);
        if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
    }
}

pub unsafe fn unregister() -> Result<(), NTSTATUS> {
    let st = PsRemoveCreateProcessNotifyRoutineEx(Some(process_notify));
    if st == STATUS_SUCCESS { Ok(()) } else { Err(st) }
}

/*---------------------------------------------------------------------------*/
/*  Utility functions                                                         */
/*---------------------------------------------------------------------------*/

/// Convert `UNICODE_STRING*` → `String`
unsafe fn uni_to_string(uni: *const UNICODE_STRING) -> String {
    if uni.is_null() {
        return String::new();
    }
    let u = &*uni;
    let len = (u.Length / 2) as usize;
    let buf = slice::from_raw_parts(u.Buffer, len);
    String::from_utf16_lossy(buf)
}

/// Convert Windows `LARGE_INTEGER` (100 ns since 1601‑01‑01) to Unix `Timestamp`
fn li_to_timestamp(li: LARGE_INTEGER) -> Timestamp {
    // 100 ns → seconds / nanos
    let ticks = li.QuadPart as i64;
    const WINDOWS_TO_UNIX_SECS: i64 = 11_644_473_600; // seconds between 1601‑01‑01 & 1970‑01‑01
    let secs  = (ticks / 10_000_000) - WINDOWS_TO_UNIX_SECS;
    let nanos = ((ticks % 10_000_000) * 100) as i32;
    Timestamp { seconds: secs, nanos }
}

/*---------------------------------------------------------------------------*/
/*  Actual callback                                                           */
/*---------------------------------------------------------------------------*/

unsafe extern "C" fn process_notify(
    parent: PEPROCESS,
    process: PEPROCESS,
    info_ptr: *mut PS_CREATE_NOTIFY_INFO,
) {
    // Exit events have NULL info_ptr
    if info_ptr.is_null() {
        return;
    }

    /*----------------------------- gather fields ----------------------------*/
    let pid  = PsGetProcessId(process) as u32;
    let ppid = PsGetProcessId(parent)  as u32;

    let info       = &*info_ptr;
    let image_path = uni_to_string(info.ImageFileName);
    let cmdline    = uni_to_string(info.CommandLine);

    // Timestamp
    let mut li = LARGE_INTEGER { QuadPart: 0 };
    KeQuerySystemTimePrecise(&mut li);
    let ts = li_to_timestamp(li);

    /*----------------------------- build messages ---------------------------*/
    let proc_evt = ProcessEvent { pid, ppid, image_path, cmdline };
    let base_evt = BaseEvent {
        ts: Some(ts),
        sensor_guid: SENSOR_GUID.into(),
        payload: Some(base_event::Payload::ProcessEvent(proc_evt)),
    };

    /*----------------------------- serialise & push -------------------------*/
    let mut buf = Vec::<u8>::with_capacity(base_evt.encoded_len());
    if base_evt.encode(&mut buf).is_err() {
        return; // encoding failed → drop event
    }

    let ring = RING;
    if !ring.is_null() {
        (*ring).push_bytes(&buf);
    }
}

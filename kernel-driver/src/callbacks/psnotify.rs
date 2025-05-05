// callbacks/psnotify.rs
//! Process creation notification handler.
//!
//! This module focuses on intercepting process creation events to extract
//! metadata (PID, parent PID, image path, etc.) for correlation and
//! monitoring purposes. It enables early detection of suspicious process trees.

use core::{ffi::c_void, mem::size_of};
use alloc::string::String;
use wdk_sys::ntddk::{PS_CREATE_NOTIFY_INFO, PEPROCESS, PsSetCreateProcessNotifyRoutineEx, PsRemoveCreateProcessNotifyRoutineEx, KeQuerySystemTimePrecise, };
use wdk_sys::{HANDLE, LARGE_INTEGER, FilterSendMessage};


// Import or bind FilterSendMessage from your communication driver/interface
unsafe extern "system" {
    fn FilterSendMessage(
        communication_port: HANDLE,
        buffer: *mut c_void,
        buffer_size: u32,
        reply_buffer: *mut c_void,
        reply_buffer_size: u32,
        return_status: *mut u32,
    ) -> u32;
}

/// Basic process event structure sent to user space
#[repr(C)]
pub struct ProcessEvent {
    pid: u32,
    ppid: u32,
    image_path: [u16; 260], // UNICODE_STRING buffer
    ts: LARGE_INTEGER,
}

static mut COMM_PORT: HANDLE = core::ptr::null_mut();

/// Register the process creation notify callback
pub fn register() -> Result<(), NTSTATUS> {
    // Optionally, establish communication port before callback
    // COMM_PORT = setup_filter_port()?;

    let status = unsafe { PsSetCreateProcessNotifyRoutineEx(Some(process_notify), 0) };
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Unregister the process creation notify callback
pub unsafe fn unregister() -> Result<(), NTSTATUS> {
    let status = PsRemoveCreateProcessNotifyRoutineEx(Some(process_notify));
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Callback for process create/exit notifications
unsafe extern "C" fn process_notify(
    parent: PEPROCESS,
    process: PEPROCESS,
    create_info: *mut PS_CREATE_NOTIFY_INFO,
) {
    // Only handle creation events
    if create_info.is_null() {
        return;
    }
    let info = &*create_info;

    // Prepare event structure
    let mut event = ProcessEvent {
        pid: wdk_sys::ntdef::HANDLE(process as isize) as u32,
        ppid: wdk_sys::ntdef::HANDLE(parent as isize) as u32,
        image_path: [0u16; 260],
        ts: LARGE_INTEGER { QuadPart: 0 },
    };

    // Timestamp
    KeQuerySystemTimePrecise(&mut event.ts);

    // Extract image path (if available)
    if !info.ImageFileName.is_null() {
        let uni = &*info.ImageFileName;
        let len = uni.Length as usize / 2;
        let src = core::slice::from_raw_parts(uni.Buffer, len);
        event.image_path[..len].copy_from_slice(src);
    }

    // Send to user mode via filter port
    let mut ret_status: u32 = 0;
    let _ = FilterSendMessage(
        COMM_PORT,
        &mut event as *mut _ as *mut c_void,
        size_of::<ProcessEvent>() as u32,
        core::ptr::null_mut(),
        0,
        &mut ret_status,
    );
}

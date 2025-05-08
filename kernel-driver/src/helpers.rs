extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::{ptr, slice};
use prost_types::Timestamp;
use wdk_sys::{HANDLE, IRP, LARGE_INTEGER, NTSTATUS, OBJECT_ATTRIBUTES, STATUS_INVALID_PARAMETER, UNICODE_STRING};
use wdk_sys::PIO_STACK_LOCATION;

unsafe extern "system" {
    fn RtlInitUnicodeString(destination_string: *mut UNICODE_STRING, source_string: *const u16);
}

/// Converts a Rust string slice into a properly initialized UNICODE_STRING.
///
/// This function converts the input string into a UTF-16 vector (with a null terminator)
/// and then calls the kernel API RtlInitUnicodeString to initialize the UNICODE_STRING.
pub fn make_unicode(s: &str) -> UNICODE_STRING {
    // Convert the Rust &str to a wide string with a null terminator.
    let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();

    // Create an uninitialized UNICODE_STRING.
    let mut unicode_string = unsafe { MaybeUninit::<UNICODE_STRING>::zeroed().assume_init() };

    // Initialize the UNICODE_STRING using the Windows kernel function.
    unsafe {
        RtlInitUnicodeString(&mut unicode_string as *mut UNICODE_STRING, wide.as_ptr());
    }

    unicode_string
}

/// Safely retrieves the current IRP stack location from an IRP.
///
/// Instead of using an assert, this function returns a Result so callers
/// can handle a malformed IRP gracefully.
///
/// # Safety
/// The caller must ensure that `irp` is a valid pointer.
pub unsafe fn io_get_current_irp_stack_location(irp: *mut IRP) -> Result<PIO_STACK_LOCATION, NTSTATUS> {
    if (*irp).CurrentLocation > (*irp).StackCount + 1 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    // Return a pointer to the field, so that the caller gets a pointer to a pointer.
    Ok((*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation)
}

/// Convert a `UNICODE_STRING*` to a Rust `String`.
///
/// # Safety
/// `uni` must be a valid, initialised pointer from the kernel.
pub unsafe fn uni_to_string(uni: *const UNICODE_STRING) -> String {
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
pub fn li_to_timestamp(li: LARGE_INTEGER) -> Timestamp {
    // SAFETY: union field access.
    let ticks = unsafe { li.QuadPart as i64 };
    const WIN_TO_UNIX_SECS: i64 = 11_644_473_600;
    let secs  = (ticks / 10_000_000) - WIN_TO_UNIX_SECS;
    let nanos = ((ticks % 10_000_000) * 100) as i32;
    Timestamp { seconds: secs, nanos }
}

/// Rust equivalent of `InitializeObjectAttributes`.
///
/// # Safety
/// Caller must provide writable, valid pointers.
pub unsafe fn initialize_object_attributes(
    obj:  *mut OBJECT_ATTRIBUTES,
    name: *mut UNICODE_STRING,
    attrs: u32,
    root: HANDLE,
    sd:   *mut core::ffi::c_void,
) {
    unsafe {
        (*obj).Length = core::mem::size_of::<OBJECT_ATTRIBUTES>() as u32;
        (*obj).RootDirectory = root;
        (*obj).Attributes = attrs;
        (*obj).ObjectName = name;
        (*obj).SecurityDescriptor = sd;
        (*obj).SecurityQualityOfService = ptr::null_mut();

    }
}

// Handle of the current process
#[inline]
pub fn nt_current_process() -> HANDLE {
    (-1isize) as HANDLE
}

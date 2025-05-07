extern crate alloc;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use wdk_sys::{
    IRP,
    NTSTATUS,
    STATUS_INVALID_PARAMETER,
    UNICODE_STRING,
};
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

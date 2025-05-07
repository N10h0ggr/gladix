//! Assorted WDK constants & helpers missing in `wdk‑sys`.

use core::ptr;
use wdk_sys::{HANDLE, OBJECT_ATTRIBUTES, UNICODE_STRING};

/*────────── missing constants ─────────*/

pub const SECTION_INHERIT_VIEW_SHARE: u32 = 1;
pub const OBJ_KERNEL_HANDLE:         u32 = 0x0000_0200;

/* Used by IRP completion when `IO_NO_INCREMENT` is absent in wdk‑sys */
pub const IO_NO_INCREMENT: i8 = 0;

/*────────── macros turned into functions ─────────*/

#[inline]
pub fn nt_current_process() -> HANDLE {
    (-1isize) as HANDLE
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
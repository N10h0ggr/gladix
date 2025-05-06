#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void, iter::once, ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use wdk_sys::{NTSTATUS, STATUS_SUCCESS, HANDLE};
use wdk_sys::ntddk::{
    InitializeObjectAttributes, NtCurrentProcess, RtlInitUnicodeString,
    ZwClose, ZwCreateSection, ZwDuplicateObject, ZwMapViewOfSection, ZwUnmapViewOfSection,
    SECTION_INHERIT_VIEW_SHARE, SECTION_ALL_ACCESS, SEC_COMMIT, OBJ_KERNEL_HANDLE,
    PAGE_READWRITE, UNICODE_STRING, OBJECT_ATTRIBUTES,
};

/// Public constants so the driver & user agent agree ---------------------------
pub const RING_NAME: &str   = r"Global\MySharedSection";
pub const RING_SIZE: usize  = 64 * 1024;

/// Very small, SPSC ring – length‑unaware (the writer wraps when space ends)
pub struct MemoryRing {
    section_handle: HANDLE,
    size: usize,
    base: *mut u8,
    write_offset: AtomicUsize,
}

impl MemoryRing {
    /// Create the named section
    pub fn create(name: &str, size: usize) -> Result<Self, NTSTATUS> {
        // build UNICODE_STRING
        let mut u_name = UNICODE_STRING::default();
        let wide: Vec<u16> = name.encode_utf16().chain(once(0)).collect();
        unsafe { RtlInitUnicodeString(&mut u_name, wide.as_ptr()) };

        // OBJ_KERNEL_HANDLE attrs
        let mut attrs = OBJECT_ATTRIBUTES::default();
        unsafe { InitializeObjectAttributes(&mut attrs, &mut u_name,
                                            OBJ_KERNEL_HANDLE, ptr::null_mut(), ptr::null_mut()) };

        // section
        let mut handle: HANDLE = ptr::null_mut();
        let mut max = size as i64;
        let st = unsafe {
            ZwCreateSection(&mut handle, SECTION_ALL_ACCESS, &mut attrs,
                            &mut max, PAGE_READWRITE, SEC_COMMIT, ptr::null_mut())
        };
        if st != STATUS_SUCCESS { return Err(st) }

        Ok(Self {
            section_handle: handle,
            size,
            base: ptr::null_mut(),
            write_offset: AtomicUsize::new(0),
        })
    }

    /// Map into kernel space so we can write
    pub fn map(&mut self) -> Result<(), NTSTATUS> {
        let mut base: *mut c_void = ptr::null_mut();
        let mut view = self.size;
        let st = unsafe {
            ZwMapViewOfSection(self.section_handle, NtCurrentProcess(), &mut base,
                               0, 0, ptr::null_mut(), &mut view,
                               SECTION_INHERIT_VIEW_SHARE, 0, PAGE_READWRITE)
        };
        if st != STATUS_SUCCESS { return Err(st) }
        self.base = base as *mut u8;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn dup_for_user(&self, target: HANDLE, opts: u32) -> Result<HANDLE, NTSTATUS> {
        let mut dup: HANDLE = ptr::null_mut();
        let st = unsafe {
            ZwDuplicateObject(NtCurrentProcess(), self.section_handle,
                              target, &mut dup, SECTION_ALL_ACCESS, 0, opts)
        };
        if st == STATUS_SUCCESS { Ok(dup) } else { Err(st) }
    }

    /// Very simple write – wraps at end of buffer, no length prefix
    pub fn push_bytes(&self, buf: &[u8]) {
        let cap = self.size;
        let mut off = self.write_offset.load(Ordering::Relaxed);
        if off + buf.len() > cap { off = 0; }
        unsafe { ptr::copy_nonoverlapping(buf.as_ptr(), self.base.add(off), buf.len()); }
        self.write_offset.store(off + buf.len(), Ordering::Relaxed);
    }
}

impl Drop for MemoryRing {
    fn drop(&mut self) {
        if !self.base.is_null() {
            unsafe { ZwUnmapViewOfSection(NtCurrentProcess(), self.base as *mut c_void); }
        }
        unsafe { ZwClose(self.section_handle); }
    }
}

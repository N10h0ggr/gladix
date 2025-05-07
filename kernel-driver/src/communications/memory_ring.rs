//! Kernel‑side ring‑buffer writer
//! ---------------------------------
//! The driver shares a named section with user‑mode.  Layout
//! (very small, single‑producer / single‑consumer, no wrap marker):
//!
//!   [u32 write_offset][ raw bytes … ]  (64 KiB total)
//!
//! * On wrap the writer simply jumps back to byte 4.
//! * Reader resets to 4 when it sees `write_offset` < current cursor.
//!
//! This module only needs **write** capability, so the mapping is
//! `PAGE_READWRITE`.

#![no_std]       // already at crate root, but harmless here
extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    iter::once,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use wdk_sys::{
    HANDLE, NTSTATUS, STATUS_SUCCESS, UNICODE_STRING, OBJECT_ATTRIBUTES,
    PAGE_READWRITE, SECTION_ALL_ACCESS, SEC_COMMIT,
    ntddk::{
        RtlInitUnicodeString, ZwClose, ZwCreateSection, ZwDuplicateObject,
        ZwMapViewOfSection, ZwUnmapViewOfSection,
    },
};

use crate::consts::{
    SECTION_INHERIT_VIEW_SHARE, nt_current_process, initialize_object_attributes,
};

/// Name and size must match the user‑mode reader.
pub const RING_NAME: &str  = r"Global\MySharedSection";
pub const RING_SIZE: usize = 64 * 1024;

/*──────────────────── struct ───────────────────*/

pub struct MemoryRing {
    section_handle: HANDLE,
    size:          usize,
    base:          *mut u8,             // mapping base in kernel
    write_offset:  AtomicUsize,         // monotonically increasing cursor
}

/*──────────────────── impl ─────────────────────*/

impl MemoryRing {
    /*—— create named section ——————————————————*/
    pub fn create(name: &str, size: usize) -> Result<Self, NTSTATUS> {
        /* UNICODE_STRING for the section name */
        let mut uni = UNICODE_STRING::default();
        let wide: Vec<u16> = name.encode_utf16().chain(once(0)).collect();
        unsafe { RtlInitUnicodeString(&mut uni, wide.as_ptr()) };

        /* OBJECT_ATTRIBUTES with OBJ_KERNEL_HANDLE */
        let mut attrs = OBJECT_ATTRIBUTES::default();
        unsafe {
            initialize_object_attributes(
                &mut attrs,
                &mut uni,
                crate::consts::OBJ_KERNEL_HANDLE,
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }

        /* ZwCreateSection */
        let mut handle: HANDLE = ptr::null_mut();
        let mut max = wdk_sys::LARGE_INTEGER { QuadPart: size as i64 };
        let st = unsafe {
            ZwCreateSection(
                &mut handle,
                SECTION_ALL_ACCESS,
                &mut attrs,
                &mut max,
                PAGE_READWRITE,
                SEC_COMMIT,
                ptr::null_mut(),
            )
        };
        if st != STATUS_SUCCESS {
            return Err(st);
        }

        Ok(Self {
            section_handle: handle,
            size,
            base: ptr::null_mut(),
            write_offset: AtomicUsize::new(0),
        })
    }

    /*—— map into kernel address space —————————*/
    pub fn map(&mut self) -> Result<(), NTSTATUS> {
        let mut base: *mut c_void = ptr::null_mut();
        let mut view_size: u64 = self.size as u64;
        let st = unsafe {
            ZwMapViewOfSection(
                self.section_handle,
                nt_current_process(),
                &mut base,
                0,
                0,
                ptr::null_mut(),
                &mut view_size,
                SECTION_INHERIT_VIEW_SHARE as i32,
                0,
                PAGE_READWRITE,
            )
        };
        if st != STATUS_SUCCESS {
            return Err(st);
        }
        self.base = base as *mut u8;
        Ok(())
    }

    /*—— duplicate handle for user‑mode (optional) —*/
    #[allow(dead_code)]
    pub fn dup_for_user(&self, target_process: HANDLE, opts: u32) -> Result<HANDLE, NTSTATUS> {
        let mut dup: HANDLE = ptr::null_mut();
        let st = unsafe {
            ZwDuplicateObject(
                nt_current_process(),
                self.section_handle,
                target_process,
                &mut dup,
                SECTION_ALL_ACCESS,
                0,
                opts,
            )
        };
        if st == STATUS_SUCCESS { Ok(dup) } else { Err(st) }
    }

    /*—— write bytes, wrapping on overflow ————*/
    /// Write bytes, wrapping at buffer end.
    /// Layout: [u32 write_off][raw bytes …].
    pub fn push_bytes(&self, buf: &[u8]) {
        let cap = self.size;
        // current writer cursor (inside the struct)
        let mut off = self.write_offset.load(Ordering::Relaxed);

        // wrap if not enough space
        if off + buf.len() > cap {
            off = 4;                        // never overwrite the cursor itself
        }

        // copy payload
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), self.base.add(off), buf.len());
        }

        let new_off = off + buf.len();

        // 1) publish to shared memory at offset 0 (little‑endian u32)
        unsafe {
            // volatile write so the reader never sees a torn value
            core::ptr::write_volatile(self.base as *mut u32, new_off as u32);
        }

        // 2) update our local cursor
        self.write_offset.store(new_off, Ordering::Release);
    }

}

/*──────────────────── Drop ─────────────────────*/

impl Drop for MemoryRing {
    fn drop(&mut self) {
        if !self.base.is_null() {
            // ignore status – nothing we can do on failure
            unsafe { let _ = ZwUnmapViewOfSection(nt_current_process(), self.base as *mut c_void); }
        }
        unsafe { let _ = ZwClose(self.section_handle); }
    }
}

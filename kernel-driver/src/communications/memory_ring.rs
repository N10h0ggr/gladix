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

extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    iter::once,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use wdk::println;
use wdk_sys::{HANDLE, NTSTATUS, STATUS_SUCCESS, UNICODE_STRING, OBJECT_ATTRIBUTES, PAGE_READWRITE, SECTION_ALL_ACCESS, SEC_COMMIT, ntddk::{
    RtlInitUnicodeString, ZwClose, ZwCreateSection, ZwDuplicateObject,
    ZwMapViewOfSection, ZwUnmapViewOfSection, MmMapViewInSystemSpace, MmUnmapViewInSystemSpace
}, SIZE_T, SECTION_MAP_READ, SECTION_MAP_WRITE, KPROCESSOR_MODE};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::ObReferenceObjectByHandle;
use crate::consts::{
    SECTION_INHERIT_VIEW_SHARE, nt_current_process, initialize_object_attributes,
};

unsafe extern "system" {
    // matches Windows’ ObfDereferenceObject signature
    fn ObDereferenceObject(ReferencedObject: *mut core::ffi::c_void);
}

/// Name and size must match the user‑mode reader.

/*──────────────────── struct ───────────────────*/

pub struct MemoryRing {
    section_handle: HANDLE,
    section_obj:    *mut core::ffi::c_void,   // <‑‑ new
    base:           *mut u8,
    size:           usize,
    write_offset:   AtomicUsize,
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
        println!("MemoryRing::create: ZwCreateSection({:?}) → {:#x}", name, st);
        if st != STATUS_SUCCESS {
            return Err(st);
        }

        Ok(Self {
            section_handle: handle,
            section_obj: ptr::null_mut(),
            size,
            base: ptr::null_mut(),
            write_offset: AtomicUsize::new(4),
        })
    }

    /*—— map into kernel address space —————————*/
    pub fn map(&mut self) -> Result<(), NTSTATUS> {
        // 1) turn the HANDLE into a real object pointer
        let mut obj: *mut core::ffi::c_void = core::ptr::null_mut();
        let status = unsafe {
            ObReferenceObjectByHandle(
                self.section_handle,
                SECTION_MAP_READ | SECTION_MAP_WRITE,   // desired access
                core::ptr::null_mut(),                  // object type = Section
                KernelMode as KPROCESSOR_MODE,
                &mut obj,
                core::ptr::null_mut(),
            )
        };
        if status != STATUS_SUCCESS {
            return Err(status);
        }

        // 2) map it in system space
        let mut base: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut view_size = self.size as SIZE_T;
        let status = unsafe {
            MmMapViewInSystemSpace(obj, &mut base, &mut view_size)
        };
        if status != STATUS_SUCCESS {
            // drop the ref before returning
            unsafe { ObDereferenceObject(obj) };
            return Err(status);
        }

        self.section_obj = obj;
        self.base = base as *mut u8;
        unsafe { core::ptr::write_volatile(self.base as *mut u32, 4) };
        Ok(())
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
            unsafe { MmUnmapViewInSystemSpace(self.base as _) };
        }
        if !self.section_obj.is_null() {
            unsafe { ObDereferenceObject(self.section_obj) };
        }
        unsafe { let _ = ZwClose(self.section_handle); }
    }
}

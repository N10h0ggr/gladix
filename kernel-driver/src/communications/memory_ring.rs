// src/communications/memory_ring.rs
#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    iter::once,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use wdk_sys::{STATUS_SUCCESS, NTSTATUS, HANDLE};
use wdk_sys::ntddk::{
    InitializeObjectAttributes, NtCurrentProcess, RtlInitUnicodeString,
    ZwClose, ZwCreateSection, ZwDuplicateObject, ZwMapViewOfSection, ZwUnmapViewOfSection,
    SECTION_INHERIT_VIEW_SHARE, SECTION_ALL_ACCESS, SEC_COMMIT, OBJ_KERNEL_HANDLE,
    PAGE_READWRITE, UNICODE_STRING, OBJECT_ATTRIBUTES,
};

/// Un simple ring buffer en una sección nombrada para compartir con user-mode.
pub struct MemoryRing {
    section_handle: HANDLE,
    size: usize,
    base: *mut u8,
    write_offset: AtomicUsize,
}

impl MemoryRing {
    /// Crea la sección nombrada de `size` bytes.
    pub fn create(name: &str, size: usize) -> Result<Self, NTSTATUS> {
        // UTF-16 + terminador
        let mut u_name = UNICODE_STRING::default();
        let wide: Vec<u16> = name.encode_utf16().chain(once(0)).collect();
        unsafe { RtlInitUnicodeString(&mut u_name, wide.as_ptr()) };

        // OBJECT_ATTRIBUTES con OBJ_KERNEL_HANDLE
        let mut obj_attrs = OBJECT_ATTRIBUTES::default();
        unsafe {
            InitializeObjectAttributes(
                &mut obj_attrs,
                &mut u_name,
                OBJ_KERNEL_HANDLE,
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }

        // ZwCreateSection
        let mut section_handle: HANDLE = ptr::null_mut();
        let mut max_size = size as i64;
        let status = unsafe {
            ZwCreateSection(
                &mut section_handle,
                SECTION_ALL_ACCESS,
                &mut obj_attrs,
                &mut max_size,
                PAGE_READWRITE,
                SEC_COMMIT,
                ptr::null_mut(),
            )
        };
        if status != STATUS_SUCCESS {
            return Err(status);
        }

        Ok(Self {
            section_handle,
            size,
            base: ptr::null_mut(),
            write_offset: AtomicUsize::new(0),
        })
    }

    /// Mapea la sección en kernel-space para poder escribir.
    pub fn map(&mut self) -> Result<(), NTSTATUS> {
        let mut base_ptr: *mut c_void = ptr::null_mut();
        let mut view_size = self.size;
        let status = unsafe {
            ZwMapViewOfSection(
                self.section_handle,
                NtCurrentProcess(),
                &mut base_ptr,
                0,
                0,
                ptr::null_mut(),
                &mut view_size,
                SECTION_INHERIT_VIEW_SHARE,
                0,
                PAGE_READWRITE,
            )
        };
        if status != STATUS_SUCCESS {
            return Err(status);
        }
        self.base = base_ptr as *mut u8;
        Ok(())
    }

    /// Duplica el handle para pasárselo a user-mode.
    pub fn dup_for_user(&self, target_process: HANDLE, options: u32) -> Result<HANDLE, NTSTATUS> {
        let mut dup: HANDLE = ptr::null_mut();
        let status = unsafe {
            ZwDuplicateObject(
                NtCurrentProcess(),
                self.section_handle,
                target_process,
                &mut dup,
                SECTION_ALL_ACCESS,
                0,
                options,
            )
        };
        if status != STATUS_SUCCESS {
            return Err(status);
        }
        Ok(dup)
    }

    /// Puntero base de la región mapeada.
    pub fn base_ptr(&self) -> *mut u8 {
        self.base
    }

    /// Capacidad total en bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Escribe `buf` en el ring, envolviendo al llegar al final.
    pub fn push_bytes(&self, buf: &[u8]) {
        let cap = self.size();
        let mut off = self.write_offset.load(Ordering::Relaxed);
        if off + buf.len() > cap {
            off = 0;
        }
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), self.base.add(off), buf.len());
        }
        self.write_offset.store(off + buf.len(), Ordering::Relaxed);
    }
}

impl Drop for MemoryRing {
    fn drop(&mut self) {
        if !self.base.is_null() {
            unsafe {
                ZwUnmapViewOfSection(NtCurrentProcess(), self.base as *mut c_void);
            }
        }
        unsafe {
            ZwClose(self.section_handle);
        }
    }
}

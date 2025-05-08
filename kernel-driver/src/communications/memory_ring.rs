//! Kernel‑side ring‑buffer writer (SPSC, length‑prefixed, no data loss)
//!
//! Layout of the mapped section (total = `HEADER_SIZE + DATA_SIZE`):
//!
//!   struct Header {
//!       AtomicU32 head;    // next free byte for producer
//!       AtomicU32 tail;    // first unread byte for consumer
//!       AtomicU32 dropped; // #events discarded for lack of space
//!       u32       size;    // DATA_SIZE (bytes) — written once
//!   }
//!   [ data[size] ]
//!
//! The producer writes `[u32 len_le][len bytes]`, wrapping naturally;
//! if `free < len+4`, it bumps `dropped` and returns.
//!
//! The consumer loads `head`, reads until `tail == head`, then
//! `tail.store(new_tail)`.  No locks, two atomics per event.

extern crate alloc;

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    iter::once,
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
use wdk::println;
use wdk_sys::{ntddk::{
    MmMapViewInSystemSpace, MmUnmapViewInSystemSpace, ObReferenceObjectByHandle, RtlInitUnicodeString,
    ZwClose, ZwCreateSection,
}, HANDLE, KPROCESSOR_MODE, NTSTATUS, OBJECT_ATTRIBUTES, PAGE_READWRITE, SEC_COMMIT, SECTION_MAP_READ, SECTION_MAP_WRITE, SIZE_T, STATUS_SUCCESS, UNICODE_STRING, STATUS_OBJECT_NAME_COLLISION, LARGE_INTEGER};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::ZwOpenSection;
use crate::helpers::initialize_object_attributes;
use crate::security::{build_world_sd, free_sd, SECTION_RWX_MASK};

unsafe extern "system" {
    fn ObDereferenceObject(ReferencedObject: *mut c_void);
}

/*──────────────── header ─────────────────*/

#[repr(C)]
struct Header {
    head:    AtomicU32,
    tail:    AtomicU32,
    dropped: AtomicU32,
    size:    u32,
}

const HEADER_SIZE: usize = size_of::<Header>();

/*──────────────── ring struct ─────────────────*/

pub struct MemoryRing {
    section_handle: HANDLE,
    section_obj:    *mut c_void,
    base:           *mut u8, // points to Header
    data_size:      usize,   // bytes in data area
}

/*──────────────── impl ─────────────────────*/

impl MemoryRing {
    pub fn create(name: &str, data_size: usize) -> Result<Self, NTSTATUS> {
        // Prepare UNICODE_STRING
        let mut uni = UNICODE_STRING::default();
        let wide: Vec<u16> = name.encode_utf16().chain(once(0)).collect();
        unsafe { RtlInitUnicodeString(&mut uni, wide.as_ptr()); }

        // Build world-security SD
        let sd = unsafe { build_world_sd()? };

        // Initialize OBJECT_ATTRIBUTES with our SD
        let mut attrs = OBJECT_ATTRIBUTES::default();
        unsafe {
            initialize_object_attributes(
                &mut attrs,
                &mut uni,
                0,
                ptr::null_mut(),
                sd.cast(),
            );
        }

        // Attempt to create or open
        let total_bytes = (size_of::<Header>() + data_size) as i64;
        // create or open the section
        let mut handle = ptr::null_mut();
        let mut max_size = LARGE_INTEGER {
            QuadPart: (size_of::<Header>() + data_size) as i64,
        };
        let status = unsafe {
            ZwCreateSection(
                &mut handle,
                SECTION_RWX_MASK,
                &mut attrs,
                &mut max_size,
                PAGE_READWRITE,
                SEC_COMMIT,
                ptr::null_mut(),
            )
        };
        unsafe { free_sd(sd as _) };  // drop our SD

        match status {
            STATUS_SUCCESS => { /* new section created */ }

            STATUS_OBJECT_NAME_COLLISION => {
                let status = unsafe {
                    ZwOpenSection(&mut handle, SECTION_RWX_MASK, &mut attrs)
                };
                if status != STATUS_SUCCESS {
                    println!("MemoryRing::create: ZwOpenSection failed {:#x}", status);
                    return Err(status);
                }
            }

            err => {
                println!("MemoryRing::create: ZwCreateSection failed {:#x}", err);
                return Err(err);
            }
        }

        println!("MemoryRing::create: got handle {:#?}", handle);
        Ok(Self { section_handle: handle, section_obj: ptr::null_mut(), base: ptr::null_mut(), data_size })

    }

    /*—— map into kernel address space —————————*/
    pub fn map(&mut self) -> Result<(), NTSTATUS> {
        // turn HANDLE into real object*
        let mut obj: *mut c_void = core::ptr::null_mut();
        let status = unsafe {
            ObReferenceObjectByHandle(
                self.section_handle,
                SECTION_MAP_READ | SECTION_MAP_WRITE,
                core::ptr::null_mut(),
                KernelMode as KPROCESSOR_MODE,
                &mut obj,
                core::ptr::null_mut(),
            )
        };
        if status != STATUS_SUCCESS {
            return Err(status);
        }

        // map it
        let mut base: *mut c_void = core::ptr::null_mut();
        let mut view_size = (HEADER_SIZE + self.data_size) as SIZE_T;
        let status = unsafe { MmMapViewInSystemSpace(obj, &mut base, &mut view_size) };
        if status != STATUS_SUCCESS {
            unsafe { ObDereferenceObject(obj) };
            return Err(status);
        }

        self.section_obj = obj;
        self.base = base as *mut u8;

        // initialise header
        let hdr = self.header_mut();
        hdr.head.store(0, Ordering::Relaxed);
        hdr.tail.store(0, Ordering::Relaxed);
        hdr.dropped.store(0, Ordering::Relaxed);
        hdr.size = self.data_size as u32;

        Ok(())
    }

    /*—— push a single message —————————————————*/
    pub fn push_bytes(&self, buf: &[u8]) {
        let hdr = self.header();
        let size = self.data_size;
        let msg_len = buf.len();

        // message too large to *ever* fit
        if msg_len + 4 > size {
            hdr.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }

        // compute free space
        let head = hdr.head.load(Ordering::Relaxed) as usize;
        let tail = hdr.tail.load(Ordering::Acquire) as usize;
        let used = if head >= tail { head - tail } else { size - (tail - head) };
        let free = size - used;

        if free < msg_len + 4 {
            hdr.dropped.fetch_add(1, Ordering::Relaxed);
            return; // drop once
        }

        // --- write [len][payload] -----------------
        let mut off = head;

        // write len (little‑endian u32)
        let len_le = (msg_len as u32).to_le_bytes();
        unsafe { ptr::copy_nonoverlapping(len_le.as_ptr(), self.data_ptr(off), 4) };
        off = (off + 4) % size;

        // write payload; may need split
        let first = core::cmp::min(msg_len, size - off);
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), self.data_ptr(off), first);
            if first < msg_len {
                ptr::copy_nonoverlapping(
                    buf.as_ptr().add(first),
                    self.data_ptr(0),
                    msg_len - first,
                );
            }
        }
        off = (off + msg_len) % size;

        // publish new head
        hdr.head.store(off as u32, Ordering::Release);
    }

    /*—— helpers ————————————————————————————*/
    #[inline]
    fn header(&self) -> &Header {
        unsafe { &*(self.base as *const Header) }
    }
    #[inline]
    fn header_mut(&self) -> &mut Header {
        unsafe { &mut *(self.base as *mut Header) }
    }
    #[inline]
    fn data_ptr(&self, off: usize) -> *mut u8 {
        unsafe { self.base.add(HEADER_SIZE + off) }
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
        unsafe { let _ = ZwClose(self.section_handle); };
    }
}

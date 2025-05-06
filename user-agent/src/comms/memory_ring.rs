//! Memory‑mapped ring‑buffer reader
//! --------------------------------
//! The kernel driver exposes a named section (`Global\MySharedSection`)
//! that contains a *single‑producer / single‑consumer* byte ring:
//
//!   ┌──── 0                             size (64 KiB) ────┐
//!   │ u32 write_off │   len | msg   len | msg   …        │
//!   └─────────────────────────────────────────────────────┘
//
//! * `write_off` **always** moves forward; the writer wraps to 4 whenever the
//!   next message would cross the buffer end.
//! * Each message is length‑prefixed (`u32`, little‑endian, unaligned).
//! * Reader keeps its own cursor (`READ_OF`, in process memory).
//!
//! This file maps the section READ‑ONLY and provides `next()` which returns the
//! next complete protobuf blob (if any).

use std::{
    ffi::OsStr,
    io,
    os::windows::prelude::OsStrExt,
    ptr,
    slice,
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::Memory::{
        OpenFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ,
        MEMORY_MAPPED_VIEW_ADDRESS,
    },
};

/// Section name chosen by the kernel driver (`ZwCreateSection`).
const SECTION_NAME: &str = r"Global\MySharedSection";
/// Size used by the driver (power‑of‑two simplifies wrap logic).
const RING_SIZE:     usize = 64 * 1024;

/// Thin RAII wrapper: maps the driver’s section on construction,
/// unmaps/close on drop.
pub struct MemoryRing {
    handle: HANDLE,                     // duplicated kernel section handle
    view:   MEMORY_MAPPED_VIEW_ADDRESS, // returned by MapViewOfFile (needed for unmap)
    base:   *mut u8,                    // raw pointer for fast offset arithmetic
    size:   usize,                      // == RING_SIZE
}

// Safe because we never mutate shared memory concurrently from multiple threads.
unsafe impl Send for MemoryRing {}
unsafe impl Sync for MemoryRing {}

impl MemoryRing {
    /// Try to open and map the named section (read‑only).
    pub fn open() -> io::Result<Self> {
        // Build null‑terminated UTF‑16 string for Win32 API
        let wide: Vec<u16> = OsStr::new(SECTION_NAME)
            .encode_wide()
            .chain(Some(0))     // NUL
            .collect();

        // OpenFileMappingW returns a HANDLE we must close later
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // MapViewOfFile → shared memory in our process
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, RING_SIZE) };
        if view.Value.is_null() {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            handle,
            view,
            base: view.Value as *mut u8,
            size: RING_SIZE,
        })
    }

    /// Return the next complete message, *if* the writer has already published it.
    /// Non‑blocking: returns `None` when there is no new data.
    pub fn next(&self) -> Option<Vec<u8>> {
        /* ── 1 ▸ observe producer cursor (Acquire) ────────────────────────── */
        let write_of = unsafe {
            (self.base as *const AtomicU32)
                .as_ref()?
                .load(Ordering::Acquire)
        } as usize;
        if write_of < 4 { return None }          // writer hasn’t initialised yet

        /* ── 2 ▸ reader cursor lives in static mut ───────────────────────── */
        static mut READ_OF: usize = 4;           // start right after write_off
        let mut ro = unsafe { READ_OF };
        if ro == write_of { return None }        // already up‑to‑date

        /* ── 3 ▸ wrap if reaching end ────────────────────────────────────── */
        if ro + 4 > self.size { ro = 4; }

        /* ── 4 ▸ read length prefix (unaligned) ───────────────────────────── */
        let len = unsafe { ptr::read_unaligned(self.base.add(ro) as *const u32) } as usize;
        if len == 0 || len > (self.size - 4) {
            // Corrupted length → resynchronise
            unsafe { READ_OF = 4 };
            return None;
        }

        /* ── 5 ▸ ensure full message already written ─────────────────────── */
        if ro + 4 + len > write_of { return None } // writer still working

        /* ── 6 ▸ copy out & advance cursor ───────────────────────────────── */
        let src = unsafe { slice::from_raw_parts(self.base.add(ro + 4), len) };
        let mut buf = vec![0u8; len];
        buf.copy_from_slice(src);

        ro += 4 + len;
        if ro >= self.size { ro = 4; }
        unsafe { READ_OF = ro };

        Some(buf)
    }
}

impl Drop for MemoryRing {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view); // unmap first
            CloseHandle(self.handle);   // then close handle
        }
    }
}

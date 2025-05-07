//! Build with `cargo run --release --bin ring_dump` and run **as Admin**.
//! Shows every `ProcessEvent` coming from the kernel driver’s ring.

use prost::Message;
use shared::events::{base_event::Payload, BaseEvent, ProcessEvent};
use std::{
    ffi::OsStr,
    os::windows::prelude::OsStrExt,
    ptr,
    slice,
    thread,
    time::Duration,
};
use windows_sys::Win32::System::Memory::{
    OpenFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ,
};

const SECTION_NAME: &str = r"Global\MySharedSection";
const RING_SIZE: usize   = 64 * 1024;

fn main() {
    /*── open section ───────────────────────────*/
    let name_utf16: Vec<u16> = OsStr::new(SECTION_NAME)
        .encode_wide()
        .chain(Some(0))
        .collect();

    let h = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name_utf16.as_ptr()) };
    if h.is_null() {
        eprintln!("❌ OpenFileMappingW failed (run as Admin & load driver)");
        return;
    }

    /*── map view ───────────────────────────────*/
    let base = unsafe {
        let p = MapViewOfFile(h, FILE_MAP_READ, 0, 0, RING_SIZE);
        if p.is_null() {
            eprintln!("❌ MapViewOfFile failed");
            return;
        }
        p as *const u8
    };

    println!("✅ Ring mapped – waiting for process events…");
    let mut ro = 4usize; // reader cursor

    loop {
        /* 1 ▸ producer cursor (volatile read) */
        let wo = unsafe { core::ptr::read_volatile(base as *const u32) } as usize;
        if ro == wo {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        if ro + 4 > RING_SIZE {
            ro = 4;
        }

        /* 2 ▸ length prefix */
        let len = unsafe { ptr::read_unaligned(base.add(ro) as *const u32) } as usize;
        if len == 0 || ro + 4 + len > wo {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        /* 3 ▸ copy protobuf bytes */
        let bytes = unsafe { slice::from_raw_parts(base.add(ro + 4), len) };

        /* 4 ▸ decode only ProcessEvent */
        if let Ok(evt) = BaseEvent::decode(bytes) {
            if let Some(Payload::ProcessEvent(ProcessEvent { pid, ppid, image_path, .. })) =
                evt.payload
            {
                println!("PROC {:5} -> {:5}  {}", ppid, pid, image_path);
            }
        }

        /* 5 ▸ advance cursor */
        ro += 4 + len;
        if ro >= RING_SIZE {
            ro = 4;
        }
    }

    // never reached
    #[allow(unreachable_code)]
    unsafe {
        let _ = UnmapViewOfFile(base as _);
    }
}

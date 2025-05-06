#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate wdk_panic;

use alloc::ffi::CString;
use core::{ptr, slice};
use core::sync::atomic::{AtomicPtr, Ordering};
use wdk::println;
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    DRIVER_OBJECT, PCUNICODE_STRING, NTSTATUS, STATUS_SUCCESS,
    ntddk::DbgPrint,
};

mod communications;
mod callbacks;

use communications::memory_ring::{MemoryRing, RING_NAME, RING_SIZE};
use callbacks::psnotify;

/// Global pointer to the ring (leaked Box so it lives for the whole driver life‑time)
static RING_PTR: AtomicPtr<MemoryRing> = AtomicPtr::new(ptr::null_mut());

/// Helper used by the callback module to get the ring
pub fn ring() -> Option<&'static MemoryRing> {
    let p = RING_PTR.load(Ordering::Acquire);
    if p.is_null() { None } else { Some(unsafe { &*p }) }
}

#[cfg(not(test))]
#[global_allocator]
static GLOBAL: WdkAllocator = WdkAllocator;

#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "C" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS
{
    // -------------------------------------------------------------------- banner
    DbgPrint(CString::new("EDR driver loading…\n").unwrap().as_ptr());
    driver.DriverUnload = Some(driver_exit);

    // -------------------------------------------------------------------- ring
    let ring = match MemoryRing::create(RING_NAME, RING_SIZE) {
        Ok(mut r) => {
            if let Err(e) = r.map() {
                println!("Ring map failed: 0x{:X}", e);
                return e;
            }
            // leak Box<MemoryRing> so we have a 'static reference everywhere
            let raw = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(r));
            RING_PTR.store(raw, Ordering::Release);
            unsafe { &*raw }
        }
        Err(e) => {
            println!("Ring create failed: 0x{:X}", e);
            return e;
        }
    };

    // -------------------------------------------------------------------- callback
    if let Err(s) = psnotify::register(ring) {
        println!("PsSetCreateProcessNotifyRoutineEx failed: 0x{:X}", s);
        return s;
    }

    // -------------------------------------------------------------------- debug path
    let reg = {
        let s = &*registry_path;
        let utf16 = slice::from_raw_parts(s.Buffer, (s.Length / 2) as usize);
        alloc::string::String::from_utf16_lossy(utf16)
    };
    println!("EDR driver ready – registry path: {}", reg);
    STATUS_SUCCESS
}

extern "C" fn driver_exit(_driver: *mut DRIVER_OBJECT) {
    println!("EDR driver unloading…");

    // 1 – unregister process callback
    unsafe { psnotify::unregister().ok(); }

    // 2 – drop ring
    let ptr = RING_PTR.swap(ptr::null_mut(), Ordering::AcqRel);
    if !ptr.is_null() {
        unsafe { alloc::boxed::Box::from_raw(ptr); }   // calls Drop, unmap + close
    }

    println!("EDR driver unloaded.");
}

//! … previous docs …

#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{boxed::Box, ffi::CString, string::String, vec::Vec};
use core::{
    ptr,
    ptr::NonNull,
    slice,
    sync::atomic::{AtomicPtr, Ordering},
};
use wdk::println;
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    DRIVER_OBJECT, PCUNICODE_STRING, PDEVICE_OBJECT,
    NTSTATUS, STATUS_SUCCESS,
    ntddk::{DbgPrint, IoCreateDevice, IoCreateSymbolicLink},
};

mod communications;
mod callbacks;
mod device;
mod consts;

use communications::memory_ring::{MemoryRing, RING_NAME, RING_SIZE};
use callbacks::psnotify;
use device::{GLADIX_TYPE, init_dispatch, delete_device};

/*------------ globals & allocator (unchanged) -------------*/

static RING_PTR: AtomicPtr<MemoryRing> = AtomicPtr::new(ptr::null_mut());

#[cfg(not(test))]
#[global_allocator]
static GLOBAL: WdkAllocator = WdkAllocator;

/*------------ UTF‑16 helper (unchanged) -------------------*/

fn make_unicode(s: &str) -> wdk_sys::UNICODE_STRING {
    let utf16: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    let mut us = wdk_sys::UNICODE_STRING::default();
    unsafe { wdk_sys::ntddk::RtlInitUnicodeString(&mut us, utf16.as_ptr()) };
    us
}

/*------------ DriverEntry -------------------------------*/

#[allow(non_snake_case)]
#[unsafe(export_name = "DriverEntry")]         // <‑‑ replace problematic #[no_mangle]
pub extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    /* banner */
    unsafe {
        DbgPrint(CString::new("Gladix driver loading…\n").unwrap().as_ptr());
    }

    /* 1 ▸ ring */
    let ring_ref: &'static MemoryRing = match MemoryRing::create(RING_NAME, RING_SIZE)
        .and_then(|mut r| { r.map()?; Ok(r) }) {
        Ok(r) => {
            let raw = Box::into_raw(Box::new(r));
            RING_PTR.store(raw, Ordering::Release);
            unsafe { &*raw }
        }
        Err(st) => return st,
    };

    /* 2 ▸ process callback */
    if let Err(st) = psnotify::register(ring_ref) {
        unsafe { drop(Box::from_raw(RING_PTR.load(Ordering::Acquire))); }
        return st;
    }

    /* 3 ▸ device object + symlink */
    let drv = unsafe { &mut *driver };
    let dev_name = make_unicode(r"\\Device\\GladixDrv");
    let mut dev_obj: PDEVICE_OBJECT = ptr::null_mut();
    let st = unsafe {
        IoCreateDevice(
            drv,
            0,
            &dev_name as *const _ as _,
            GLADIX_TYPE,
            0,
            0,
            &mut dev_obj,
        )
    };
    if st != STATUS_SUCCESS {
        unsafe { psnotify::unregister().ok(); }
        unsafe { drop(Box::from_raw(RING_PTR.load(Ordering::Acquire))); }
        return st;
    }

    // dispatch table
    unsafe { init_dispatch(driver) };

    let sym_name = make_unicode(r"\\DosDevices\\Gladix");
    unsafe { IoCreateSymbolicLink(&sym_name as *const _ as _, &dev_name as *const _ as _); }

    /* 4 ▸ registry path (debug) */
    let reg_path = unsafe {
        let us = &*registry_path;
        let utf16 = slice::from_raw_parts(us.Buffer, (us.Length / 2) as usize);
        String::from_utf16_lossy(utf16)
    };
    println!("Driver loaded. Registry path: {}", reg_path);

    // store unload routine
    unsafe { (*driver).DriverUnload = Some(driver_exit) };

    STATUS_SUCCESS
}

/*------------ unload ------------------------------------*/

extern "C" fn driver_exit(driver: *mut DRIVER_OBJECT) {
    println!("Driver unloading…");

    unsafe {
        // 1 ▸ unregister process callback
        psnotify::unregister().ok();

        // 2 ▸ delete symbolic link + device
        let sym = make_unicode(r"\\DosDevices\\Gladix");
        wdk_sys::ntddk::IoDeleteSymbolicLink(&sym as *const _ as _);
        delete_device((*driver).DeviceObject);

        // 3 ▸ drop ring
        if let Some(raw) = NonNull::new(RING_PTR.swap(ptr::null_mut(), Ordering::AcqRel)) {
            drop(Box::from_raw(raw.as_ptr()));
        }
    }

    println!("Driver unloaded.");
}

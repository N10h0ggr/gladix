//! kernel-driver/src/lib.rs
#![no_std]

extern crate alloc;
#[cfg(not(test))] extern crate wdk_panic;

use alloc::{boxed::Box, string::String};
use core::{
    ptr,
    slice,
    sync::atomic::{AtomicPtr, Ordering},
};
use wdk::println;
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    DRIVER_OBJECT, PCUNICODE_STRING, PDEVICE_OBJECT, NTSTATUS, STATUS_SUCCESS,
    FILE_DEVICE_UNKNOWN, IRP_MJ_READ, IRP_MJ_WRITE, IRP_MJ_CREATE,
    IRP_MJ_CLOSE, IRP_MJ_DEVICE_CONTROL,
};
use wdk_sys::ntddk::{IoCreateDevice, IoCreateSymbolicLink, IoDeleteDevice, IoDeleteSymbolicLink};

use shared::constants::{RING_SIZE, RING_NAME};
#[global_allocator]
static ALLOCATOR: WdkAllocator = WdkAllocator;

/*---------------- modules ----------------*/
mod communications;
mod callbacks;   // psnotify
mod dispatch;
mod helpers;
mod consts;

use communications::memory_ring::{MemoryRing};
use callbacks::psnotify;
use dispatch::*;
use helpers::*;

/*---------------- globals ----------------*/
static mut DISPATCHER: Dispatcher = Dispatcher::new();

//
// Device Extension Structure
//
// This structure is allocated per-device and holds our timer, DPC,
// spin lock, and a counter that is updated by the DPC.
#[repr(C)]
pub struct DeviceExtension {
    ring_ptr: *mut MemoryRing,
}

/*---------------- DriverEntry ----------------*/


#[unsafe(no_mangle)]         // exports "DriverEntry"
pub unsafe extern "C" fn DriverEntry(
    driver: *mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("DriverEntry: Gladix loading...");

    /* 1 ─ ring section ------------------------------------------------------*/
    // TODO: Handle when Shared Section already exists.
    let raw_ring  = match MemoryRing::create(RING_NAME, RING_SIZE)
        .and_then(|mut r| { r.map()?; Ok(r) })
    {
        Ok(r) => Box::into_raw(Box::new(r)),
        Err(st) => return st,
    };
    let ring_ref = unsafe { &*raw_ring };

    /* 2 ─ process‑notify callback ------------------------------------------*/
    // TODO: Check if process_notify function is beeing called
    if let Err(st) = unsafe { psnotify::register(ring_ref) } {
        unsafe { drop(Box::from_raw(raw_ring)); }
        return st;
    }
    println!("DriverEntry: ring and callbacks OK");

    /* 3 ─ device object -----------------------------------------------------*/
    let dev_name = make_unicode(r"\Device\GladixDrv");
    let mut dev_obj: PDEVICE_OBJECT = ptr::null_mut();
    let st = unsafe {
        IoCreateDevice(
            driver,
            size_of::<DeviceExtension>() as u32,
            &dev_name as *const _ as _,
            FILE_DEVICE_UNKNOWN,
            0,
            0u8,                       // exclusive = FALSE
            &mut dev_obj,
        )
    };
    if st != STATUS_SUCCESS {
        unsafe {
            psnotify::unregister().ok();
            drop(Box::from_raw(raw_ring));
        }
        return st;
    }
    println!("DriverEntry: device created");

    let dev_ext = unsafe {
        &mut *((*dev_obj).DeviceExtension.cast::<DeviceExtension>())
    };
    dev_ext.ring_ptr = raw_ring;

    /* 4 ─ dispatch table ----------------------------------------------------*/
    unsafe {
        let disp: *mut Dispatcher = core::ptr::addr_of_mut!(DISPATCHER); // raw ptr – avoids &mut
        (*disp).register(IRP_MJ_READ,            dispatch_read);
        (*disp).register(IRP_MJ_WRITE,           dispatch_write);
        (*disp).register(IRP_MJ_CREATE,          dispatch_create);
        (*disp).register(IRP_MJ_CLOSE,           dispatch_close);
        (*disp).register(IRP_MJ_DEVICE_CONTROL,  dispatch_device_control);
        (*disp).install(driver);
    }

    /* 5 ─ symbolic link -----------------------------------------------------*/
    let sym = make_unicode(r"\DosDevices\Gladix");
    unsafe { IoCreateSymbolicLink(&sym as *const _ as _, &dev_name as *const _ as _) };
    println!("DriverEntry: IOCTL interface ready");

    /* 6 ─ debug registry path ----------------------------------------------*/
    let reg = unsafe {
        let us = &*registry_path;
        let w  = slice::from_raw_parts(us.Buffer, (us.Length / 2) as usize);
        String::from_utf16_lossy(w)
    };
    println!("DriverEntry complete. Registry = {}", reg);

    unsafe { (*driver).DriverUnload = Some(driver_unload) };
    STATUS_SUCCESS
}

/*──────── unload ────────*/
extern "C" fn driver_unload(driver: *mut DRIVER_OBJECT) {
    println!("Gladix: unloading...");
    unsafe {
        psnotify::unregister().ok();

        let sym = make_unicode(r"\DosDevices\Gladix");
        IoDeleteSymbolicLink(&sym as *const _ as _);
        IoDeleteDevice((*driver).DeviceObject);

        let dev_obj = (*driver).DeviceObject;
        if !dev_obj.is_null() {
            let dev_ext = &mut *((*dev_obj).DeviceExtension.cast::<DeviceExtension>());
            let raw_ring = dev_ext.ring_ptr;
            if !raw_ring.is_null() {
                drop(Box::from_raw(raw_ring));
            }
        }
    }
    println!("Gladix: unloaded.");
}

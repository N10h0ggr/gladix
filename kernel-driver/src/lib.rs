//! Entry point and initialization for the EDR kernel-mode driver.
//!
//! This file defines the entry point (`DriverEntry`) and unload routine for the
//! kernel driver. It is responsible for setting up global state, initializing
//! kernel-mode subsystems (e.g., minifilters, WFP callouts, callbacks), and
//! providing clean teardown during driver unload.
//!
//! Key responsibilities:
//! - Serve as the central integration point for all driver components.
//! - Call initialization routines for subsystems.
//! - Handle global registration (minifilter, WFP, callbacks).
//! - Ensure orderly shutdown and resource release on unload.
//!
//! This file is compiled with `#![no_std]` and assumes proper configuration for
//! Windows kernel-mode development.

#![no_std]
extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{ffi::CString, slice, string::String};

use wdk::println;
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{ntddk::DbgPrint, DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING, STATUS_SUCCESS};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

/// `driver_entry` function required by WDM
///
/// # Panics
/// Can panic from unwraps of `CStrings` used internally
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it from WDM
#[export_name = "DriverEntry"]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // This is an example of directly using DbgPrint binding to print
    let string = CString::new("Hello World!\n").unwrap();
    unsafe {
        DbgPrint(string.as_ptr());
    }

    driver.DriverUnload = Some(driver_exit);

    // Translate UTF16 string to rust string
    let registry_path: String = String::from_utf16_lossy(unsafe {
        slice::from_raw_parts(
            (*registry_path).Buffer,
            (*registry_path).Length as usize / core::mem::size_of_val(&(*(*registry_path).Buffer)),
        )
    });

    // It is much better to use the println macro that has an implementation in
    // wdk::print.rs to call DbgPrint. The println! implementation in
    // wdk::print.rs has the same features as the one in std (ex. format args
    // support).
    println!("WDM Driver Entry Complete! Driver Registry Parameter Key: {registry_path}");

    STATUS_SUCCESS
}

extern "C" fn driver_exit(_driver: *mut DRIVER_OBJECT) {
    println!("Goodbye World!");
    println!("Driver Exit Complete!");
}
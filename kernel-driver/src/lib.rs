#![no_std]
extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{ffi::CString, string::String, vec::Vec};
use core::slice;

use wdk::println;
use wdk_sys::{
    DRIVER_OBJECT,
    PCUNICODE_STRING,
    PS_CREATE_NOTIFY_INFO,
    PEPROCESS,
    UNICODE_STRING,
    STATUS_SUCCESS,
    NTSTATUS,
    HANDLE,
    ntddk::{PsSetCreateProcessNotifyRoutineEx, DbgPrint, PsGetProcessId},
};
use wdk_alloc::WdkAllocator;

use prost::Message;
use shared::events::ProcessEvent;

mod communications;
use communications::memory_ring::MemoryRing;

static mut SHARED_RING: Option<MemoryRing> = None;

/// Callback: serializa el evento y lo escribe en el ring.
unsafe extern "C" fn process_notify_ring(
    parent: PEPROCESS,
    process: PEPROCESS,
    info_ptr: *mut PS_CREATE_NOTIFY_INFO,
) {
    if info_ptr.is_null() {
        return;
    }
    let info = &*info_ptr;

    // Obtenemos PID y PPID con PsGetProcessId
    let pid  = PsGetProcessId(process) as u32;
    let ppid = PsGetProcessId(parent)  as u32;

    // Convierte UNICODE_STRING a Rust String
    fn uni_to_string(us: &UNICODE_STRING) -> String {
        let len = (us.Length / 2) as usize;
        let buf = unsafe { slice::from_raw_parts(us.Buffer, len) };
        String::from_utf16_lossy(buf)
    }

    let image_path = if !info.ImageFileName.is_null() {
        uni_to_string(&*info.ImageFileName)
    } else {
        String::new()
    };
    let cmdline = if !info.CommandLine.is_null() {
        uni_to_string(&*info.CommandLine)
    } else {
        String::new()
    };

    // Serializamos el mensaje protobuf
    let evt = ProcessEvent { pid, ppid, image_path, cmdline };
    let mut buf: Vec<u8> = Vec::with_capacity(evt.encoded_len());
    if evt.encode(&mut buf).is_err() {
        return;
    }

    // Lo empujamos al ring
    if let Some(ring) = SHARED_RING.as_ref() {
        ring.push_bytes(&buf);
    }
}

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "C" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // Debug print
    let banner = CString::new("EDR Driver Loading...\n").unwrap();
    DbgPrint(banner.as_ptr());

    // Unload handler
    driver.DriverUnload = Some(driver_exit);

    // 1) Crear y mapear ring (64 KiB)
    match MemoryRing::create(r"Global\MySharedSection", 64 * 1024) {
        Ok(mut ring) => {
            if let Err(e) = ring.map() {
                println!("Error mapeando ring: 0x{:X}", e);
                return e;
            }
            SHARED_RING = Some(ring);
        }
        Err(e) => {
            println!("Error creando ring: 0x{:X}", e);
            return e;
        }
    }

    // 2) Registrar callback de creación de procesos
    let status = PsSetCreateProcessNotifyRoutineEx(Some(process_notify_ring), false);
    if status != STATUS_SUCCESS {
        println!("Falló registro de notify: 0x{:X}", status);
        return status;
    }

    // 3) Debug: imprimimos la ruta del registry
    let reg = {
        let us = &*registry_path;
        let sl = slice::from_raw_parts(us.Buffer, (us.Length / 2) as usize);
        String::from_utf16_lossy(sl)
    };
    println!("EDR Driver listo. Registry Path: {}", reg);

    STATUS_SUCCESS
}

extern "C" fn driver_exit(_driver: *mut DRIVER_OBJECT) {
    println!("EDR Driver Unloading...");

    unsafe {
        // Removemos callback
        let _ = PsSetCreateProcessNotifyRoutineEx(Some(process_notify_ring), true);
        // Al asignar None, se invoca Drop en MemoryRing (unmap + close)
        SHARED_RING = None;
    }

    println!("EDR Driver Unloaded.");
}

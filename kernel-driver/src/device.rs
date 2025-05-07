//! Gladix mini‑device: IRP dispatcher and helpers.
//!
//!  * \Device\GladixDrv is created in `lib.rs`.
//!  * Only CREATE/CLOSE succeed today; everything else is
//!    `STATUS_INVALID_DEVICE_REQUEST` until real IOCTLs are added.

#![allow(non_upper_case_globals)]

use wdk_sys::{
    // I/O major codes
    IRP_MJ_CLOSE, IRP_MJ_CREATE, IRP_MJ_DEVICE_CONTROL,
    // object types
    PDEVICE_OBJECT, PIRP, PIO_STACK_LOCATION, IRP,
    // NT status
    NTSTATUS, STATUS_SUCCESS, STATUS_INVALID_DEVICE_REQUEST, STATUS_INVALID_PARAMETER,
    ntddk::{IofCompleteRequest, IoDeleteDevice}
    // KM API
};

use crate::consts::IO_NO_INCREMENT;

/*──────────────────────────────── device type ───────────────────────────*/

/// Custom device type in user‑defined range (0x8000‑0xFFFF).
pub const GLADIX_TYPE: u32 = 0x8000;

/*──────────────────────────── IRP helpers ───────────────────────────────*/

/// Finish an IRP and return the given status.
///
/// # Safety
///   * `irp` must point to a valid IRP.
///   * Caller must guarantee IRQL == `DISPATCH_LEVEL` or lower.
pub unsafe fn complete(irp: PIRP, status: NTSTATUS) -> NTSTATUS {
    // union field produced by bindgen
    (*irp).IoStatus.__bindgen_anon_1.Status = status as _;
    (*irp).IoStatus.Information = 0;
    // second param is CCHAR (i8)
    IofCompleteRequest(irp, IO_NO_INCREMENT);
    status
}

/// Safe wrapper that validates `CurrentLocation`.
///
/// Returns a **pointer to** the current `IO_STACK_LOCATION`.
///
/// # Safety
/// Caller guarantees `irp` is valid.
pub unsafe fn io_get_current_irp_stack_location(
    irp: *mut IRP,
) -> Result<PIO_STACK_LOCATION, NTSTATUS> {
    unsafe {
        if (*irp).CurrentLocation > (*irp).StackCount + 1 {
            return Err(STATUS_INVALID_PARAMETER);
        }
        Ok((*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation)
    }
}

/*──────────────────────────── dispatch table ────────────────────────────*/

unsafe extern "C" fn dispatch_ok(_dev: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    // body is safe, call to `complete` must still be wrapped
    unsafe { complete(irp, STATUS_SUCCESS) }
}

unsafe extern "C" fn dispatch_invalid(_dev: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    unsafe { complete(irp, STATUS_INVALID_DEVICE_REQUEST) }
}

/// Install major‑function pointers.
///
/// # Safety
/// Must be called exactly once, with the same `driver_object` that owns
/// the device created in `lib.rs`.
pub unsafe fn init_dispatch(driver_object: *mut wdk_sys::DRIVER_OBJECT) {
    let drv = &mut *driver_object;
    drv.MajorFunction[IRP_MJ_CREATE as usize]         = Some(dispatch_ok);
    drv.MajorFunction[IRP_MJ_CLOSE  as usize]         = Some(dispatch_ok);
    drv.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(dispatch_invalid);
}

/*────────────────────────── unload helper ──────────────────────────────*/

/// Delete device object (called from `driver_exit`).
///
/// # Safety
/// `dev` must have been created by `IoCreateDevice`.
pub unsafe fn delete_device(dev: PDEVICE_OBJECT) {
    IoDeleteDevice(dev);
}
